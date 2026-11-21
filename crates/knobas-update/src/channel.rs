//! The two channels and where their manifest is (decisions D4).
//!
//! *Releases* reads `<web>/BFoerschner/knobas/releases/latest/download/latest.json`: GitHub's
//! *latest* is never a draft or a pre-release, and the address touches no rate limit. *Pre-release
//! builds* asks `<api>/repos/BFoerschner/knobas/releases?per_page=10`, keeps the published entries
//! whose tag is a version and that carry a `latest.json`, and reads the greatest one's manifest by
//! knobas's order. Both end in a manifest in hand.

use std::cmp::Ordering;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::manifest::{self, Manifest};
use crate::version;

/// The repository whose releases knobas reads.
pub const REPO: &str = "BFoerschner/knobas";
/// The most bytes a manifest or a release list may have.
pub const ANSWER_CAP: u64 = 4 << 20;
/// The most bytes of a release's notes kept.
pub const NOTES_CAP: usize = 200 * 1024;

/// Which versions knobas offers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "update.ts")]
pub enum Channel {
    /// Finished releases, the default.
    #[default]
    Releases,
    /// The newest published release or pre-release, whichever is newer.
    PreReleases,
}

/// Where GitHub is. A test points both at a loopback server (decisions D16): that changes where
/// knobas asks, never what it trusts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    /// `https://github.com`.
    pub web: String,
    /// `https://api.github.com`.
    pub api: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            web: "https://github.com".to_owned(),
            api: "https://api.github.com".to_owned(),
        }
    }
}

impl Endpoints {
    /// GitHub's, or what `KNOBAS_UPDATE_WEB` and `KNOBAS_UPDATE_API` say.
    ///
    /// # Errors
    ///
    /// The variable whose value is neither https nor http to a loopback address.
    pub fn from_env(web: Option<String>, api: Option<String>) -> Result<Self, String> {
        let mut endpoints = Self::default();
        for (name, value, slot) in [
            ("KNOBAS_UPDATE_WEB", web, &mut endpoints.web),
            ("KNOBAS_UPDATE_API", api, &mut endpoints.api),
        ] {
            let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
                continue;
            };
            let parsed = url::Url::parse(value.trim())
                .map_err(|error| format!("{name}={value} is not a URL: {error}"))?;
            if !allowed(&parsed, true) {
                return Err(format!(
                    "{name}={value} is neither https nor http to this machine (127.0.0.1, ::1, localhost)"
                ));
            }
            *slot = value.trim().trim_end_matches('/').to_owned();
        }
        Ok(endpoints)
    }

    /// The release page of `version`.
    #[must_use]
    pub fn release_page(&self, version: &str) -> String {
        format!("{}/{REPO}/releases/tag/v{version}", self.web)
    }

    /// The releases' page.
    #[must_use]
    pub fn releases_page(&self) -> String {
        format!("{}/{REPO}/releases", self.web)
    }
}

/// Whether `url` may be asked: https, or http to a loopback address when the reads started on
/// this machine (`sources/remote.rs`'s rule for a test forge).
#[must_use]
pub fn allowed(url: &url::Url, from_this_machine: bool) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => from_this_machine && on_this_machine(url),
        _ => false,
    }
}

/// Whether `url`'s host is a loopback address or `localhost`.
#[must_use]
pub fn on_this_machine(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        Some(url::Host::Domain(name)) => name.eq_ignore_ascii_case("localhost"),
        None => false,
    }
}

/// Why a GET did not answer with a body.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    #[error("not found")]
    NotFound,
    #[error("GitHub's limit of requests for this address is used up")]
    RateLimited { reset: Option<DateTime<Utc>> },
    #[error("{0}")]
    Offline(String),
    #[error("{0}")]
    Bad(String),
}

/// One GET, for a body of at most `cap` bytes. The app's is `crate::fetch::HttpFetch`; a test's
/// records what it was asked.
#[async_trait::async_trait]
pub trait Fetch: Send + Sync {
    async fn get(&self, url: &str, cap: u64) -> Result<Vec<u8>, FetchError>;
}

/// Why a check found nothing to say.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CheckError {
    #[error("{0}")]
    Offline(String),
    #[error("GitHub's limit of requests for this address is used up")]
    RateLimited { reset: Option<DateTime<Utc>> },
    #[error("{0}")]
    BadAnswer(String),
}

impl From<FetchError> for CheckError {
    fn from(error: FetchError) -> Self {
        match error {
            FetchError::Offline(why) => Self::Offline(why),
            FetchError::RateLimited { reset } => Self::RateLimited { reset },
            FetchError::NotFound => Self::BadAnswer("GitHub answered 404".to_owned()),
            FetchError::Bad(why) => Self::BadAnswer(why),
        }
    }
}

/// A channel's newest version, its manifest in hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub manifest: Manifest,
    pub pre_release: bool,
    pub published_at: Option<DateTime<Utc>>,
    pub page_url: String,
    /// The notes, when the answer that found the version already held them.
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct ApiRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    published_at: Option<DateTime<Utc>>,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<ApiAsset>,
}

/// The channel's newest version, or `None` when it carries no manifest.
///
/// # Errors
///
/// [`CheckError`].
pub async fn discover(
    channel: Channel,
    fetch: &dyn Fetch,
    endpoints: &Endpoints,
) -> Result<Option<Found>, CheckError> {
    match channel {
        Channel::Releases => {
            let url = format!(
                "{}/{REPO}/releases/latest/download/latest.json",
                endpoints.web
            );
            let body = match fetch.get(&url, ANSWER_CAP).await {
                Ok(body) => body,
                // The newest release carries no manifest: nothing newer.
                Err(FetchError::NotFound) => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            let manifest = parse(&body)?;
            let published_at = manifest
                .pub_date
                .as_deref()
                .and_then(|date| DateTime::parse_from_rfc3339(date).ok())
                .map(|date| date.with_timezone(&Utc));
            Ok(Some(Found {
                pre_release: version::is_pre_release(&manifest.version),
                published_at,
                page_url: endpoints.release_page(&manifest.version),
                notes: None,
                manifest,
            }))
        }
        Channel::PreReleases => {
            let url = format!("{}/repos/{REPO}/releases?per_page=10", endpoints.api);
            let body = fetch.get(&url, ANSWER_CAP).await?;
            let releases: Vec<ApiRelease> = serde_json::from_slice(&body)
                .map_err(|error| CheckError::BadAnswer(format!("the release list: {error}")))?;
            let Some((release, version, asset)) = newest_with_manifest(&releases) else {
                return Ok(None);
            };
            let body = match fetch.get(asset, ANSWER_CAP).await {
                Ok(body) => body,
                Err(FetchError::NotFound) => return Ok(None),
                Err(error) => return Err(error.into()),
            };
            let manifest = parse(&body)?;
            let tagged = version.to_string();
            if version::parse(&manifest.version).map(|v| v.to_string()) != Some(tagged.clone()) {
                return Err(CheckError::BadAnswer(format!(
                    "the latest.json of {} names version {}",
                    release.tag_name, manifest.version
                )));
            }
            Ok(Some(Found {
                pre_release: release.prerelease,
                published_at: release.published_at,
                page_url: release
                    .html_url
                    .clone()
                    .unwrap_or_else(|| endpoints.release_page(&tagged)),
                notes: release.body.as_deref().map(notes),
                manifest,
            }))
        }
    }
}

/// Of the published entries whose tag is a version and that carry a `latest.json`, the greatest
/// by knobas's order, with its version and the manifest's address.
fn newest_with_manifest(releases: &[ApiRelease]) -> Option<(&ApiRelease, version::Version, &str)> {
    releases
        .iter()
        .filter(|release| !release.draft && release.published_at.is_some())
        .filter_map(|release| {
            let version = version::parse(&release.tag_name)?;
            if !release.tag_name.starts_with('v') {
                return None;
            }
            let asset = release
                .assets
                .iter()
                .find(|asset| asset.name == "latest.json")?;
            Some((release, version, asset.browser_download_url.as_str()))
        })
        .max_by(|a, b| version::order(&a.1, &b.1).then(Ordering::Equal))
}

fn parse(body: &[u8]) -> Result<Manifest, CheckError> {
    let text = std::str::from_utf8(body)
        .map_err(|_| CheckError::BadAnswer("latest.json is not UTF-8".to_owned()))?;
    manifest::parse(text).map_err(|error| CheckError::BadAnswer(error.to_string()))
}

/// A release's body up to its first line that is `---`, at most [`NOTES_CAP`] bytes.
#[must_use]
pub fn notes(body: &str) -> String {
    let mut kept = String::new();
    let mut lines = body
        .lines()
        .skip_while(|line| line.trim().is_empty())
        .peekable();
    // The release's own "## What's new in vX.Y.Z": the page heads the notes with it already.
    if lines.peek().is_some_and(|line| {
        line.trim_start_matches('#')
            .trim()
            .starts_with("What's new in")
    }) {
        lines.next();
    }
    for line in lines {
        if line.trim() == "---" {
            break;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    if kept.len() > NOTES_CAP {
        let mut cut = NOTES_CAP;
        while !kept.is_char_boundary(cut) {
            cut -= 1;
        }
        kept.truncate(cut);
    }
    kept.trim().to_owned()
}

/// The notes of `version` and its page, from `<api>/repos/…/releases/tags/v<version>`.
///
/// # Errors
///
/// [`CheckError`].
pub async fn release_notes(
    fetch: &dyn Fetch,
    endpoints: &Endpoints,
    version: &str,
) -> Result<(Option<String>, String), CheckError> {
    let url = format!("{}/repos/{REPO}/releases/tags/v{version}", endpoints.api);
    let body = fetch.get(&url, ANSWER_CAP).await?;
    let release: ApiRelease = serde_json::from_slice(&body)
        .map_err(|error| CheckError::BadAnswer(format!("the release: {error}")))?;
    let page = release
        .html_url
        .clone()
        .unwrap_or_else(|| endpoints.release_page(version));
    Ok((release.body.as_deref().map(notes), page))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    /// A fetcher that answers from a table and records every URL.
    #[derive(Default)]
    struct Table {
        answers: HashMap<String, Result<String, FetchError>>,
        asked: Mutex<Vec<String>>,
    }

    impl Table {
        fn with(mut self, url: &str, answer: Result<String, FetchError>) -> Self {
            self.answers.insert(url.to_owned(), answer);
            self
        }
        fn asked(&self) -> Vec<String> {
            self.asked.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl Fetch for Table {
        async fn get(&self, url: &str, _cap: u64) -> Result<Vec<u8>, FetchError> {
            self.asked.lock().unwrap().push(url.to_owned());
            match self.answers.get(url) {
                Some(Ok(body)) => Ok(body.clone().into_bytes()),
                Some(Err(error)) => Err(error.clone()),
                None => Err(FetchError::NotFound),
            }
        }
    }

    const LATEST: &str =
        "https://github.com/BFoerschner/knobas/releases/latest/download/latest.json";
    const LIST: &str = "https://api.github.com/repos/BFoerschner/knobas/releases?per_page=10";

    fn manifest(version: &str) -> String {
        format!(r#"{{"version":"{version}","pub_date":"2026-11-22T09:41:07Z","platforms":{{}}}}"#)
    }

    fn release(
        tag: &str,
        draft: bool,
        pre: bool,
        published: Option<&str>,
        manifest: bool,
    ) -> serde_json::Value {
        let mut assets =
            vec![serde_json::json!({ "name": "x.exe", "browser_download_url": "https://x/x.exe" })];
        if manifest {
            assets.push(serde_json::json!({ "name": "latest.json", "browser_download_url": format!("https://dl/{tag}/latest.json") }));
        }
        serde_json::json!({
            "tag_name": tag, "draft": draft, "prerelease": pre, "published_at": published,
            "html_url": format!("https://github.com/BFoerschner/knobas/releases/tag/{tag}"),
            "body": format!("## What's new in {tag}\n\nThings.\n\n---\nunsigned"),
            "assets": assets,
        })
    }

    #[tokio::test]
    async fn releases_reads_latest_json_and_nothing_else() {
        let fetch = Table::default().with(LATEST, Ok(manifest("3.2.9")));
        let found = discover(Channel::Releases, &fetch, &Endpoints::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.manifest.version, "3.2.9");
        assert!(!found.pre_release);
        assert_eq!(
            found.page_url,
            "https://github.com/BFoerschner/knobas/releases/tag/v3.2.9"
        );
        assert_eq!(fetch.asked(), vec![LATEST.to_owned()]);
        assert!(fetch.asked().iter().all(|url| !url.contains("api.")));
    }

    #[tokio::test]
    async fn a_404_on_releases_is_nothing_newer() {
        let fetch = Table::default();
        assert_eq!(
            discover(Channel::Releases, &fetch, &Endpoints::default()).await,
            Ok(None)
        );
    }

    /// Mutants: take the first entry (the draft or the manifest-less one wins); order by
    /// `published_at` (the hotfix release published last wins, in the next test).
    #[tokio::test]
    async fn pre_releases_takes_the_greatest_version_that_carries_a_manifest() {
        let list = serde_json::json!([
            release("v3.2.9-ux30", true, true, None, true),
            release(
                "v3.2.9-ux29",
                false,
                true,
                Some("2026-11-23T00:00:00Z"),
                false
            ),
            release("v3.2.8", false, false, Some("2026-09-01T00:00:00Z"), true),
            release(
                "v3.2.9-ux28",
                false,
                true,
                Some("2026-11-22T00:00:00Z"),
                true
            ),
        ]);
        let fetch = Table::default().with(LIST, Ok(list.to_string())).with(
            "https://dl/v3.2.9-ux28/latest.json",
            Ok(manifest("3.2.9-ux28")),
        );
        let found = discover(Channel::PreReleases, &fetch, &Endpoints::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.manifest.version, "3.2.9-ux28");
        assert!(found.pre_release);
        assert_eq!(found.notes.as_deref(), Some("Things."));
    }

    #[tokio::test]
    async fn a_hotfix_published_after_a_newer_pre_release_does_not_win() {
        let list = serde_json::json!([
            release("v3.2.8", false, false, Some("2026-11-24T00:00:00Z"), true),
            release(
                "v3.2.9-ux9",
                false,
                true,
                Some("2026-11-20T00:00:00Z"),
                true
            ),
            release(
                "v3.2.9-ux10",
                false,
                true,
                Some("2026-11-21T00:00:00Z"),
                true
            ),
        ]);
        let fetch = Table::default().with(LIST, Ok(list.to_string())).with(
            "https://dl/v3.2.9-ux10/latest.json",
            Ok(manifest("3.2.9-ux10")),
        );
        let found = discover(Channel::PreReleases, &fetch, &Endpoints::default())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(found.manifest.version, "3.2.9-ux10");
    }

    #[tokio::test]
    async fn a_manifest_of_another_version_than_its_tag_is_refused() {
        let list = serde_json::json!([release(
            "v3.2.9-ux28",
            false,
            true,
            Some("2026-11-22T00:00:00Z"),
            true
        )]);
        let fetch = Table::default().with(LIST, Ok(list.to_string())).with(
            "https://dl/v3.2.9-ux28/latest.json",
            Ok(manifest("3.2.9-ux27")),
        );
        assert!(matches!(
            discover(Channel::PreReleases, &fetch, &Endpoints::default()).await,
            Err(CheckError::BadAnswer(_))
        ));
    }

    #[tokio::test]
    async fn a_rate_limited_answer_carries_when_to_try_again() {
        let reset = DateTime::from_timestamp(1_800_000_000, 0);
        let fetch = Table::default().with(LIST, Err(FetchError::RateLimited { reset }));
        assert_eq!(
            discover(Channel::PreReleases, &fetch, &Endpoints::default()).await,
            Err(CheckError::RateLimited { reset })
        );
    }

    /// Decisions D16. Mutant: accept plain http anywhere.
    #[test]
    fn an_endpoint_that_is_neither_https_nor_loopback_is_refused() {
        assert!(Endpoints::from_env(Some("http://example.com".into()), None).is_err());
        assert!(Endpoints::from_env(None, Some("ftp://127.0.0.1".into())).is_err());
        let local = Endpoints::from_env(
            Some("http://127.0.0.1:8080/".into()),
            Some("https://api.example".into()),
        )
        .unwrap();
        assert_eq!(local.web, "http://127.0.0.1:8080");
        assert_eq!(local.api, "https://api.example");
        assert_eq!(
            Endpoints::from_env(None, None).unwrap(),
            Endpoints::default()
        );
    }

    #[test]
    fn notes_stop_at_the_rule() {
        assert_eq!(notes("## What's new in v3.2.9\n\nA\n---\nB"), "A");
        assert_eq!(notes("## Fixed\n\nA\n---\nB"), "## Fixed\n\nA");
        let long = "x".repeat(NOTES_CAP + 10);
        assert_eq!(notes(&long).len(), NOTES_CAP);
    }
}
