//! A checkout brought up to the mirror's tip behind the reader (ADR-0041).
//!
//! *Files* reads a branch's tree at its checkout when the checkout holds the
//! commit the mirror names for the branch ([`super::local`]). One that does
//! not (the person has not fetched since it was pushed) is read from the
//! source as before, and meanwhile that one commit is fetched into the
//! checkout's object store; once it has landed, [`Fetched`] tells the shell
//! (`checkout:fetched`) and the next read is local.
//!
//! # What runs (ADR-0041, amending ADR-0023 and ADR-0036)
//!
//! One command, at the checkout:
//!
//! ```text
//! git -c protocol.allow=never -c protocol.https.allow=always -c protocol.ssh.allow=always
//!     -c submodule.recurse=false -C <checkout>
//!     fetch --no-tags --no-recurse-submodules --no-auto-maintenance
//!     --no-write-fetch-head --quiet -- <url> <sha>
//! ```
//!
//! - **No ref, no `FETCH_HEAD`, no working tree.** With no destination and
//!   `--no-write-fetch-head` git writes the commit's objects and nothing
//!   else: the person's branches, remote-tracking refs, index and files are
//!   as they were, and nothing shows in their `git log --all`. Unreferenced,
//!   the objects go with git's own pruning, as a pull request's worktree's
//!   would.
//! - **The URL is *Clone*'s** ([`super::clone::url_of`]): the source's typed
//!   address and the checked `owner/name`, never a mirrored URL.
//! - **The one argument from the mirror is a checked sha** (ADR-0034's
//!   grammar, 40 or 64 hex digits here): the branch's tip the mirror holds.
//!   A branch name never reaches git.
//! - **The person's credentials,** as for *Clone*; no prompt
//!   (`GIT_TERMINAL_PROMPT=0`).
//! - **One at a time per checkout, and at most once a minute per commit:** a
//!   fetch that failed is not tried again for every tree read.

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use knobas_core::checkout::open::is_sha;
use knobas_sync::scheduler::AdapterRegistry;
use serde::Serialize;
use sqlx::PgPool;

/// How long one fetch may take.
pub const TIMEOUT: Duration = Duration::from_secs(120);

/// How long a fetch of one commit is not tried again after it ran.
pub const AGAIN_AFTER: Duration = Duration::from_secs(60);

/// The `checkout:fetched` payload: a fetch behind a read landed, and the
/// tree of `entity_id` can now be read at its checkout.
///
/// The smoothness pass (PR #2479, ADR-0041).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Fetched {
    pub entity_id: String,
    /// The commit that landed.
    pub commit: String,
}

type Listener = Arc<dyn Fn(Fetched) + Send + Sync>;

/// The fetches behind reads, one registry per app.
pub struct Fetches {
    running: Mutex<HashSet<PathBuf>>,
    tried: Mutex<HashMap<(PathBuf, String), Instant>>,
    listener: Mutex<Option<Listener>>,
    protocols: Vec<&'static str>,
}

impl Default for Fetches {
    fn default() -> Self {
        Self {
            running: Mutex::default(),
            tried: Mutex::default(),
            listener: Mutex::default(),
            protocols: knobas_core::checkout::clone::PROTOCOLS.to_vec(),
        }
    }
}

impl Fetches {
    /// The one registry the app's commands use.
    #[must_use]
    pub fn global() -> Arc<Fetches> {
        static GLOBAL: LazyLock<Arc<Fetches>> = LazyLock::new(Arc::default);
        Arc::clone(&GLOBAL)
    }

    /// A registry admitting `protocols` instead of https and ssh. **Tests
    /// only**, as [`super::clone::start_admitting`]: the testenv Gitea speaks
    /// http.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn admitting(protocols: &[&'static str]) -> Self {
        Self {
            protocols: protocols.to_vec(),
            ..Self::default()
        }
    }

    /// Who is told of a fetch that landed: the bridge, in the app.
    pub fn on_fetched(&self, listener: impl Fn(Fetched) + Send + Sync + 'static) {
        *self.listener.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(listener));
    }

    /// Fetch `sha` into the checkout at `path` of `entity_id`'s repository,
    /// in the background, unless one runs there already or this commit was
    /// tried within [`AGAIN_AFTER`]. Answers at once.
    pub async fn behind(
        self: &Arc<Self>,
        pool: &PgPool,
        registry: &dyn AdapterRegistry,
        entity_id: &str,
        path: &str,
        sha: &str,
    ) {
        if !is_sha(sha) || sha.len() < 40 {
            return;
        }
        let path = PathBuf::from(path);
        let key = (path.clone(), sha.to_owned());
        {
            let mut tried = self.tried.lock().unwrap_or_else(PoisonError::into_inner);
            tried.retain(|_, at| at.elapsed() < AGAIN_AFTER);
            if tried.contains_key(&key) {
                return;
            }
            let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
            if !running.insert(path.clone()) {
                return;
            }
            tried.insert(key, Instant::now());
        }
        let release = {
            let me = Arc::clone(self);
            let path = path.clone();
            move || {
                me.running
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&path);
            }
        };
        let Ok(Some(repo)) = super::repo_of(pool, entity_id).await.map(|r| r.entity_id) else {
            release();
            return;
        };
        let url = match super::clone::url_of(pool, registry, &repo, &self.protocols).await {
            Ok((url, _)) => url,
            Err(error) => {
                tracing::debug!(%entity_id, %error, "no address to fetch the tip from");
                release();
                return;
            }
        };
        let me = Arc::clone(self);
        let (entity_id, sha) = (entity_id.to_owned(), sha.to_owned());
        tokio::spawn(async move {
            let landed = fetch(&path, &url, &sha, &me.protocols).await;
            release();
            match landed {
                Ok(()) => {
                    let listener = me
                        .listener
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .clone();
                    if let Some(listener) = listener {
                        listener(Fetched {
                            entity_id,
                            commit: sha,
                        });
                    }
                }
                Err(why) => tracing::info!(%entity_id, %why, "the tip could not be fetched"),
            }
        });
    }
}

/// The argument list of the one fetch (ADR-0041), pinned by the tests.
#[must_use]
pub fn args(checkout: &Path, url: &str, sha: &str, protocols: &[&str]) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec!["-c".into(), "protocol.allow=never".into()];
    for protocol in protocols {
        args.push("-c".into());
        args.push(format!("protocol.{protocol}.allow=always").into());
    }
    args.extend(["-c", "submodule.recurse=false", "-C"].map(OsString::from));
    args.push(checkout.as_os_str().to_owned());
    args.extend(
        [
            "fetch",
            "--no-tags",
            "--no-recurse-submodules",
            "--no-auto-maintenance",
            "--no-write-fetch-head",
            "--quiet",
            "--",
            url,
            sha,
        ]
        .map(OsString::from),
    );
    args
}

/// Run the fetch, and say whether the checkout now holds `sha`.
async fn fetch(path: &Path, url: &str, sha: &str, protocols: &[&str]) -> Result<(), String> {
    let git = super::working_tree::git_on_path()
        .await
        .map_err(|why| format!("{why:?}"))?;
    super::worktree::run(&git, args(path, url, sha, protocols), TIMEOUT).await?;
    if super::range::holds_commits(path, &[sha]).await {
        Ok(())
    } else {
        Err(format!("git fetched, and the checkout still lacks {sha}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_argument_list_is_the_adrs() {
        let args = args(
            Path::new("/code/payout-service"),
            "https://git.example/tidewater/payout-service.git",
            "0123456789abcdef0123456789abcdef01234567",
            &["https", "ssh"],
        );
        let shown: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            shown.join(" "),
            "-c protocol.allow=never -c protocol.https.allow=always \
             -c protocol.ssh.allow=always -c submodule.recurse=false \
             -C /code/payout-service fetch --no-tags --no-recurse-submodules \
             --no-auto-maintenance --no-write-fetch-head --quiet -- \
             https://git.example/tidewater/payout-service.git \
             0123456789abcdef0123456789abcdef01234567"
        );
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=knobas test",
                "-c",
                "user.email=test@knobas.invalid",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// The fetch lands the commit's objects and writes no ref, no
    /// `FETCH_HEAD`, no index and no file of the checkout.
    #[tokio::test]
    async fn a_fetch_lands_the_commit_and_touches_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let upstream = dir.path().join("upstream");
        std::fs::create_dir(&upstream).unwrap();
        git(&upstream, &["init", "-q", "-b", "main"]);
        std::fs::write(upstream.join("a.txt"), "one\n").unwrap();
        git(&upstream, &["add", "."]);
        git(&upstream, &["commit", "-q", "-m", "one"]);
        let checkout = dir.path().join("checkout");
        git(
            dir.path(),
            &["clone", "-q", &upstream.to_string_lossy(), "checkout"],
        );
        std::fs::write(upstream.join("a.txt"), "two\n").unwrap();
        git(&upstream, &["commit", "-q", "-am", "two"]);
        let tip = git(&upstream, &["rev-parse", "HEAD"]);
        assert!(!super::super::range::holds_commits(&checkout, &[&tip]).await);

        let refs = git(&checkout, &["for-each-ref"]);
        let index = std::fs::read(checkout.join(".git/index")).unwrap();
        let url = format!("file://{}", upstream.display());
        fetch(&checkout, &url, &tip, &["file"])
            .await
            .expect("the fetch lands");

        assert!(super::super::range::holds_commits(&checkout, &[&tip]).await);
        assert_eq!(git(&checkout, &["for-each-ref"]), refs);
        assert_eq!(std::fs::read(checkout.join(".git/index")).unwrap(), index);
        assert!(!checkout.join(".git/FETCH_HEAD").exists());
        assert_eq!(
            std::fs::read_to_string(checkout.join("a.txt")).unwrap(),
            "one\n"
        );
    }
}
