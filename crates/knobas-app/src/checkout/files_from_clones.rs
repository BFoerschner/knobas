//! The files pass's reader of checkouts (the everything-search decisions,
//! 2026-11-20, D3): a blob from a checkout of the repository on this
//! machine, found as *Files* finds it ([`super::resolve`]), read in process
//! (ADR-0043). No request and no process: only the blobs the last fetch did
//! not bring go to the forge.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sqlx::PgPool;

/// How long a repository's checkout, or its lack of one, is believed: a
/// pass asks the same repository for each of its files.
const BELIEVED: Duration = Duration::from_secs(300);

/// [`knobas_sync::code_files::Clones`] over the checkouts this machine holds.
pub struct FromClones {
    pool: PgPool,
    paths: Mutex<HashMap<String, (Instant, Option<PathBuf>)>>,
}

impl FromClones {
    /// Over the profile's database, where the clones root and each
    /// repository's override are read.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            paths: Mutex::new(HashMap::new()),
        }
    }

    /// Where `repository` is checked out on this machine, if it is.
    async fn path(&self, repository: &str) -> Option<PathBuf> {
        if let Some((at, path)) = self
            .paths
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(repository)
            && at.elapsed() < BELIEVED
        {
            return path.clone();
        }
        let path = super::resolve(&self.pool, repository)
            .await
            .ok()
            .and_then(|view| view.path)
            .map(PathBuf::from);
        self.paths
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(repository.to_owned(), (Instant::now(), path.clone()));
        path
    }
}

impl knobas_sync::code_files::Clones for FromClones {
    fn blob<'a>(
        &'a self,
        repository: &'a str,
        sha: &'a str,
        max: usize,
    ) -> Pin<Box<dyn Future<Output = Option<Vec<u8>>> + Send + 'a>> {
        Box::pin(async move {
            let path = self.path(repository).await?;
            let (bytes, cut) = super::local::blob(&path, sha, max).await?;
            (!cut).then_some(bytes)
        })
    }

    fn holds<'a>(&'a self, repository: &'a str) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        Box::pin(async move { self.path(repository).await.is_some() })
    }
}
