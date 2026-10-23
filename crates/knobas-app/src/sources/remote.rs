//! Where an external adapter's release is (ADR-0028, issue #1209): a local
//! manifest, a manifest URL, or a GitHub repository URL that resolves to its
//! latest release's `knobas-adapter.json`.
//!
//! [`Origin::parse`] reads what the reader pasted, and what
//! `knobas.external_adapter.source_url` stored, into one of three shapes;
//! [`locate`] finds a release's manifest from it, the latest one or the one
//! a tag names, and [`artifact`] fetches the component the manifest names.
//!
//! **A release's tag.** A GitHub repository's release answers its own
//! `tag_name`. A manifest URL of the shape every forge's release downloads
//! have (`<forge>/<owner>/<repo>/releases/download/<tag>/<file>`, GitHub,
//! Gitea and Forgejo alike) carries its tag in the path, and its forge's
//! release API says which release is the latest: GitHub's
//! `/repos/<o>/<r>/releases/latest`, Gitea's and Forgejo's
//! `/api/v1/repos/<o>/<r>/releases/latest`, one shape of answer. Any other
//! URL, and a local file, is no release: it has no tag, and its hash alone
//! pins it.
//!
//! The fetches go through `knobas-http`, the one HTTP stack, with no
//! credential: a release is public, and a private one is out of scope.

use std::path::PathBuf;

use knobas_http::{HttpClient, HttpConfig, Method};
use knobas_source::SourceError;

/// The largest manifest knobas reads.
const MANIFEST_CAP: usize = 1 << 20;
/// The largest artifact knobas downloads.
const ARTIFACT_CAP: usize = 64 << 20;

/// Where GitHub is: the web host a repository URL names, and the API that
/// answers for it. A test points both at a local server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// The host of a repository URL, `github.com`.
    pub github_host: String,
    /// The REST API's base, `https://api.github.com`.
    pub github_api: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            github_host: "github.com".to_owned(),
            github_api: "https://api.github.com".to_owned(),
        }
    }
}

/// Where a release is, read from what the reader pasted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A `knobas-adapter.json` on this machine, by absolute path.
    Local(PathBuf),
    /// `https://github.com/<owner>/<repo>`: its latest release.
    GitHubRepo { owner: String, repo: String },
    /// A manifest URL; a forge's release download when [`release_of`] reads
    /// one in it.
    Manifest(url::Url),
}

/// Which release [`locate`] reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pin<'a> {
    /// The latest one (install, the update check, update).
    Latest,
    /// The one the install recorded (restore), or, with no tag, whatever the
    /// origin names.
    Recorded(Option<&'a str>),
}

/// A release's manifest, found.
#[derive(Debug, Clone)]
pub struct Located {
    /// The release's tag; `None` for a local manifest or a URL that is no
    /// release.
    pub tag: Option<String>,
    /// The manifest's text, unparsed.
    pub manifest: String,
    /// Where the artifact a manifest names is fetched from.
    assets: Assets,
}

#[derive(Debug, Clone)]
enum Assets {
    /// Beside a local manifest.
    Directory(PathBuf),
    /// Relative to the manifest's URL.
    Beside(url::Url),
    /// A release's asset list, by name.
    Listed(Vec<Asset>),
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, serde::Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// Why a release could not be found or read. The message is the reader's.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("{0} is neither a manifest URL, a GitHub repository URL nor a local file")]
    NotAnOrigin(String),
    #[error("cannot read the manifest {path}: {error}")]
    Unreadable { path: String, error: std::io::Error },
    #[error("cannot fetch {url}: {error}")]
    Unreachable { url: String, error: SourceError },
    #[error("{url} is larger than {cap} bytes")]
    TooLarge { url: String, cap: usize },
    #[error("the release {tag} has no {file}")]
    NoAsset { tag: String, file: String },
    #[error("the release answer from {url} is not a release: {error}")]
    NotARelease { url: String, error: String },
}

impl Origin {
    /// Read `input`: an `http(s)` URL is a GitHub repository when it is one
    /// on [`Endpoints::github_host`] with an owner and a name and nothing
    /// after them, else a manifest URL; anything else is a local path.
    ///
    /// # Errors
    ///
    /// [`FetchError::NotAnOrigin`] for an empty input or a URL that does
    /// not parse.
    pub fn parse(input: &str, endpoints: &Endpoints) -> Result<Self, FetchError> {
        let input = input.trim();
        let not = || FetchError::NotAnOrigin(format!("{input:?}"));
        if input.is_empty() {
            return Err(not());
        }
        let lower = input.to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return std::path::absolute(input)
                .map(Origin::Local)
                .map_err(|_| not());
        }
        let url = url::Url::parse(input).map_err(|_| not())?;
        if url.host_str() == Some(endpoints.github_host.as_str()) && url.query().is_none() {
            let parts: Vec<&str> = url.path().split('/').filter(|s| !s.is_empty()).collect();
            if let [owner, repo] = parts.as_slice() {
                return Ok(Origin::GitHubRepo {
                    owner: (*owner).to_owned(),
                    repo: repo.trim_end_matches(".git").to_owned(),
                });
            }
        }
        Ok(Origin::Manifest(url))
    }

    /// How the install records it in `source_url`, which [`Origin::parse`]
    /// reads back to this origin.
    #[must_use]
    pub fn record(&self, endpoints: &Endpoints) -> String {
        match self {
            Origin::Local(path) => path.display().to_string(),
            Origin::GitHubRepo { owner, repo } => {
                format!("https://{}/{owner}/{repo}", endpoints.github_host)
            }
            Origin::Manifest(url) => url.to_string(),
        }
    }
}

/// A forge's release download: its release API (`…/releases`, no slash)
/// and the URL's tag and file. `None` for a URL of any other shape.
#[must_use]
pub fn release_of(url: &url::Url, endpoints: &Endpoints) -> Option<(String, String, String)> {
    let parts: Vec<&str> = url.path_segments()?.collect();
    let n = parts.len();
    if n < 6 || parts[n - 4] != "releases" || parts[n - 3] != "download" || url.query().is_some() {
        return None;
    }
    let (owner, repo, tag, file) = (parts[n - 6], parts[n - 5], parts[n - 2], parts[n - 1]);
    if [owner, repo, tag, file].iter().any(|s| s.is_empty()) {
        return None;
    }
    let api = if url.host_str() == Some(endpoints.github_host.as_str()) && n == 6 {
        format!("{}/repos/{owner}/{repo}/releases", endpoints.github_api)
    } else {
        let prefix = parts[..n - 6].join("/");
        let mut base = url.clone();
        base.set_path(if prefix.is_empty() { "" } else { &prefix });
        let base = base.as_str().trim_end_matches('/').to_owned();
        format!("{base}/api/v1/repos/{owner}/{repo}/releases")
    };
    let decode = |s: &str| {
        percent_encoding::percent_decode_str(s)
            .decode_utf8_lossy()
            .into_owned()
    };
    Some((api, decode(tag), decode(file)))
}

/// Find the manifest of the release `pin` names at `origin`.
///
/// # Errors
///
/// [`FetchError`] naming what could not be read or reached.
pub async fn locate(
    origin: &Origin,
    pin: Pin<'_>,
    endpoints: &Endpoints,
) -> Result<Located, FetchError> {
    match origin {
        Origin::Local(path) => {
            let manifest =
                tokio::fs::read_to_string(path)
                    .await
                    .map_err(|error| FetchError::Unreadable {
                        path: path.display().to_string(),
                        error,
                    })?;
            let dir = path.parent().map(PathBuf::from).unwrap_or_default();
            Ok(Located {
                tag: None,
                manifest,
                assets: Assets::Directory(dir),
            })
        }
        Origin::GitHubRepo { owner, repo } => {
            let api = format!("{}/repos/{owner}/{repo}/releases", endpoints.github_api);
            from_release_api(&api, pin, knobas_wasm_host::manifest::FILE_NAME).await
        }
        Origin::Manifest(url) => match (release_of(url, endpoints), pin) {
            (Some((api, _, file)), Pin::Latest) => from_release_api(&api, pin, &file).await,
            (Some((api, recorded, file)), Pin::Recorded(Some(tag))) if tag != recorded => {
                from_release_api(&api, pin, &file).await
            }
            (release, _) => {
                let manifest = text(&get(url.as_str(), MANIFEST_CAP).await?, url.as_str())?;
                Ok(Located {
                    tag: release.map(|(_, tag, _)| tag),
                    manifest,
                    assets: Assets::Beside(url.clone()),
                })
            }
        },
    }
}

/// The release `pin` names through a forge's release API, and its
/// manifest, the asset named `file`.
async fn from_release_api(api: &str, pin: Pin<'_>, file: &str) -> Result<Located, FetchError> {
    let url = match pin {
        Pin::Latest | Pin::Recorded(None) => format!("{api}/latest"),
        Pin::Recorded(Some(tag)) => format!("{api}/tags/{}", encode_segment(tag)),
    };
    let body = get(&url, MANIFEST_CAP).await?;
    let release: Release =
        serde_json::from_slice(&body).map_err(|error| FetchError::NotARelease {
            url: url.clone(),
            error: error.to_string(),
        })?;
    let asset = find(&release.assets, &release.tag_name, file)?;
    let manifest = text(&get(&asset, MANIFEST_CAP).await?, &asset)?;
    Ok(Located {
        tag: Some(release.tag_name),
        manifest,
        assets: Assets::Listed(release.assets),
    })
}

fn find(assets: &[Asset], tag: &str, file: &str) -> Result<String, FetchError> {
    assets
        .iter()
        .find(|asset| asset.name == file)
        .map(|asset| asset.browser_download_url.clone())
        .ok_or_else(|| FetchError::NoAsset {
            tag: tag.to_owned(),
            file: file.to_owned(),
        })
}

/// The bytes of the artifact `file` a release's manifest names: beside a
/// local manifest, beside a manifest URL, or the release's asset of that
/// name. The name is a bare file name: `Manifest::parse` refused any other.
///
/// # Errors
///
/// [`FetchError`] naming what could not be read or reached.
pub async fn artifact(located: &Located, file: &str) -> Result<Vec<u8>, FetchError> {
    match &located.assets {
        Assets::Directory(dir) => {
            let path = dir.join(file);
            tokio::fs::read(&path)
                .await
                .map_err(|error| FetchError::Unreadable {
                    path: path.display().to_string(),
                    error,
                })
        }
        Assets::Beside(manifest) => {
            let url =
                manifest
                    .join(&encode_segment(file))
                    .map_err(|e| FetchError::Unreachable {
                        url: format!("{manifest} beside {file}"),
                        error: SourceError::protocol(e.to_string()),
                    })?;
            get(url.as_str(), ARTIFACT_CAP).await
        }
        Assets::Listed(assets) => {
            let tag = located.tag.as_deref().unwrap_or_default();
            get(&find(assets, tag, file)?, ARTIFACT_CAP).await
        }
    }
}

/// `segment` as one path segment: everything but the unreserved
/// characters escaped, so a tag holding `/`, `?` or `#` stays one segment.
fn encode_segment(segment: &str) -> String {
    const SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'.')
        .remove(b'_')
        .remove(b'~');
    percent_encoding::utf8_percent_encode(segment, SEGMENT).to_string()
}

fn text(bytes: &[u8], url: &str) -> Result<String, FetchError> {
    String::from_utf8(bytes.to_vec()).map_err(|_| FetchError::NotARelease {
        url: url.to_owned(),
        error: "the manifest is not UTF-8".to_owned(),
    })
}

/// GET `url` through the one HTTP stack, with no credential, refusing a
/// body over `cap` bytes.
async fn get(url: &str, cap: usize) -> Result<Vec<u8>, FetchError> {
    let unreachable = |error| FetchError::Unreachable {
        url: url.to_owned(),
        error,
    };
    let parsed = url::Url::parse(url)
        .map_err(|e| unreachable(SourceError::protocol(format!("not a URL: {e}"))))?;
    let origin = parsed.origin().ascii_serialization();
    let mut path = parsed.path().to_owned();
    if let Some(query) = parsed.query() {
        path = format!("{path}?{query}");
    }
    let max_body = u64::try_from(cap).unwrap_or(u64::MAX);
    let client = HttpClient::new(HttpConfig {
        base_url: origin,
        adapter_kind: "adapter-install".to_owned(),
        adapter_version: env!("CARGO_PKG_VERSION").to_owned(),
        // This read's own cap, which for a component is over the client's
        // default (#1640): the client counts the body against it as it
        // arrives, and a body that runs past it is `TooLarge` below.
        max_body,
        ..HttpConfig::default()
    })
    .map_err(unreachable)?;
    let response = client
        .send(client.request(Method::GET, &path))
        .await
        .map_err(unreachable)?;
    let too_large = || FetchError::TooLarge {
        url: url.to_owned(),
        cap,
    };
    if response.content_length().is_some_and(|n| n > max_body) {
        return Err(too_large());
    }
    let bytes = response.bytes().await.map_err(|e| {
        if knobas_http::over_the_cap(&e) {
            too_large()
        } else {
            unreachable(SourceError::Unreachable(format!("reading the body: {e}")))
        }
    })?;
    Ok(bytes.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Origin {
        Origin::parse(input, &Endpoints::default()).unwrap()
    }

    #[test]
    fn a_github_repository_url_is_a_repository_with_or_without_its_git_suffix() {
        for input in [
            "https://github.com/harbour/surveys",
            "https://github.com/harbour/surveys/",
            "https://github.com/harbour/surveys.git",
        ] {
            assert_eq!(
                parse(input),
                Origin::GitHubRepo {
                    owner: "harbour".to_owned(),
                    repo: "surveys".to_owned()
                },
                "{input}"
            );
        }
    }

    #[test]
    fn any_other_url_is_a_manifest_and_anything_else_a_local_file() {
        let url = "https://github.com/harbour/surveys/releases/download/v1/knobas-adapter.json";
        assert!(matches!(parse(url), Origin::Manifest(_)));
        assert!(matches!(
            parse("https://forge.example/m.json"),
            Origin::Manifest(_)
        ));
        assert!(matches!(
            parse("/tmp/knobas-adapter.json"),
            Origin::Local(_)
        ));
        assert!(Origin::parse("  ", &Endpoints::default()).is_err());
    }

    #[test]
    fn a_recorded_origin_reads_back_as_itself() {
        let endpoints = Endpoints::default();
        for input in [
            "https://github.com/harbour/surveys",
            "https://forge.example/harbour/surveys/releases/download/v1/knobas-adapter.json",
            "/tmp/knobas-adapter.json",
        ] {
            let origin = parse(input);
            assert_eq!(
                Origin::parse(&origin.record(&endpoints), &endpoints).unwrap(),
                origin
            );
        }
    }

    fn release(url: &str) -> Option<(String, String, String)> {
        release_of(&url::Url::parse(url).unwrap(), &Endpoints::default())
    }

    #[test]
    fn a_github_release_download_asks_github_s_api() {
        assert_eq!(
            release(
                "https://github.com/harbour/surveys/releases/download/v1.2.0/knobas-adapter.json"
            ),
            Some((
                "https://api.github.com/repos/harbour/surveys/releases".to_owned(),
                "v1.2.0".to_owned(),
                "knobas-adapter.json".to_owned()
            ))
        );
    }

    #[test]
    fn a_forge_release_download_asks_its_own_api_under_its_path_prefix() {
        assert_eq!(
            release("https://forge.example/git/harbour/surveys/releases/download/v%201&b+c/m.json"),
            Some((
                "https://forge.example/git/api/v1/repos/harbour/surveys/releases".to_owned(),
                "v 1&b+c".to_owned(),
                "m.json".to_owned()
            ))
        );
        assert_eq!(
            release("http://127.0.0.1:3000/harbour/surveys/releases/download/v1/m.json")
                .map(|r| r.0),
            Some("http://127.0.0.1:3000/api/v1/repos/harbour/surveys/releases".to_owned())
        );
    }

    /// One answer of `len` bytes from a thread, with no `content-length`, so
    /// only counting the body can find its size.
    fn undeclared_body(len: usize) -> String {
        use std::io::{Read as _, Write as _};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept");
            let _ = socket.read(&mut [0u8; 4096]);
            let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nconnection: close\r\n\r\n");
            let _ = socket.write_all(&vec![b'x'; len]);
        });
        format!("http://127.0.0.1:{port}/knobas-adapter.wasm")
    }

    /// A component may be larger than the client's default body cap, up to
    /// this read's own (#1640): the client is built with this read's cap.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_component_over_the_default_body_cap_is_fetched_whole() {
        let len = usize::try_from(knobas_http::MAX_BODY).unwrap() + 1024 * 1024;
        assert!(len < ARTIFACT_CAP);
        let bytes = get(&undeclared_body(len), ARTIFACT_CAP)
            .await
            .expect("fetched");
        assert_eq!(bytes.len(), len);
    }

    /// A body that runs past this read's cap is `TooLarge`, declared or not.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_body_over_the_reads_cap_is_too_large() {
        let error = get(&undeclared_body(MANIFEST_CAP + 1), MANIFEST_CAP)
            .await
            .expect_err("over the cap");
        assert!(
            matches!(error, FetchError::TooLarge { cap, .. } if cap == MANIFEST_CAP),
            "{error:?}"
        );
    }

    #[test]
    fn a_url_of_any_other_shape_is_no_release() {
        for url in [
            "https://forge.example/knobas-adapter.json",
            "https://github.com/harbour/surveys/releases/latest/download/knobas-adapter.json",
            "https://forge.example/harbour/surveys/releases/download/v1/m.json?x=1",
        ] {
            assert_eq!(release(url), None, "{url}");
        }
    }
}
