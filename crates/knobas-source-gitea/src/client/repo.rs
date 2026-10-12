//! The reads a branch's and a repository's source views make (#663), beside
//! the ones the sync makes. Each is one request; none is stored (ADR-0020
//! rule 1).

use knobas_http::Method;
use knobas_source::SourceError;

use super::{API_ROOT, GiteaClient, page_params, segment};

/// Generic over the record, so each view keeps its own partial projection
/// beside the code that reads it.
impl GiteaClient {
    /// `GET /repos/{o}/{r}`, as the view's own projection.
    pub(crate) async fn repo_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<T, SourceError> {
        self.get_json(
            &format!("/repos/{}/{}", segment(owner)?, segment(repo)?),
            &[],
        )
        .await
    }

    /// `GET /repos/{o}/{r}/branches/{branch}` -- one branch and its tip.
    pub(crate) async fn branch_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<T, SourceError> {
        self.get_json(
            &format!(
                "/repos/{}/{}/branches/{}",
                segment(owner)?,
                segment(repo)?,
                escaped(branch)?
            ),
            &[],
        )
        .await
    }

    /// `GET /repos/{o}/{r}/compare/{base}...{head}` -- the commits `head`
    /// carries that `base` does not, and how many.
    pub(crate) async fn compare_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        base: &str,
        head: &str,
    ) -> Result<T, SourceError> {
        self.get_json(&compare_path(owner, repo, base, head)?, &[])
            .await
    }

    /// The same compare as `git diff` prints it (`?output=diff`, Gitea
    /// 1.27). The API's route, not the web UI's `compare/…diff`: that one
    /// ignores a token and answers 404 for a private repository (measured on
    /// 1.27.3, 2026-10-12).
    pub(crate) async fn compare_diff(
        &self,
        owner: &str,
        repo: &str,
        base: &str,
        head: &str,
    ) -> Result<String, SourceError> {
        self.get_text_with(
            &compare_path(owner, repo, base, head)?,
            &[("output", "diff".to_owned())],
        )
        .await
    }

    /// One page of a repository's **open** pull requests, most recently
    /// updated first.
    pub(crate) async fn open_pulls_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        page: u32,
    ) -> Result<Vec<T>, SourceError> {
        let (limit, page) = page_params(page);
        self.get_json(
            &format!("/repos/{}/{}/pulls", segment(owner)?, segment(repo)?),
            &[
                ("state", "open".to_owned()),
                ("sort", "recentupdate".to_owned()),
                limit,
                page,
            ],
        )
        .await
    }

    /// The newest `limit` commits of `branch`, the listing projections off.
    /// An empty repository's 409 is no commits, as for [`Self::commits`].
    pub(crate) async fn latest_commits_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        limit: u32,
    ) -> Result<Vec<T>, SourceError> {
        let path = format!("/repos/{}/{}/commits", segment(owner)?, segment(repo)?);
        let query = [
            ("sha", branch.to_owned()),
            ("stat", "false".to_owned()),
            ("verification", "false".to_owned()),
            ("files", "false".to_owned()),
            ("limit", limit.to_string()),
            ("page", "1".to_owned()),
        ];
        match self.get_json(&path, &query).await {
            Err(error) if error.status() == Some(409) => Ok(Vec::new()),
            other => other,
        }
    }

    /// `GET /repos/{o}/{r}/contents` -- the default branch's top directory.
    /// Gitea 1.27 has no `/readme` route (it answers 404 *page not found*),
    /// so this is how a README is found.
    pub(crate) async fn root_contents_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
    ) -> Result<Vec<T>, SourceError> {
        self.get_json(
            &format!("/repos/{}/{}/contents", segment(owner)?, segment(repo)?),
            &[],
        )
        .await
    }

    /// `GET /repos/{o}/{r}/raw/{file}` -- one top-level file, as text.
    pub(crate) async fn raw_file(
        &self,
        owner: &str,
        repo: &str,
        file: &str,
    ) -> Result<String, SourceError> {
        self.get_text_with(
            &format!(
                "/repos/{}/{}/raw/{}",
                segment(owner)?,
                segment(repo)?,
                escaped(file)?
            ),
            &[],
        )
        .await
    }

    /// `GET <base>/api/v1/<path>?<query>`, its body as text: a `.diff`, a
    /// raw file. A refusal is the error it maps to, never an empty text.
    pub(super) async fn get_text_with(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<String, SourceError> {
        let path = format!("{API_ROOT}{path}");
        let request = self.http.request(Method::GET, &path).query(query);
        let response = self.http.send(request).await?;
        response.text().await.map_err(|error| {
            SourceError::protocol(format!(
                "gitea: {path} answered an unreadable body: {error}"
            ))
        })
    }
}

/// `/repos/{o}/{r}/compare/{base}...{head}`, both refs escaped.
fn compare_path(owner: &str, repo: &str, base: &str, head: &str) -> Result<String, SourceError> {
    Ok(format!(
        "/repos/{}/{}/compare/{}...{}",
        segment(owner)?,
        segment(repo)?,
        escaped(base)?,
        escaped(head)?
    ))
}

/// A branch or file name as **one** path segment: every byte but git's
/// unreserved ones percent-encoded, `/` included (Gitea 1.27 resolves
/// `feature%2Fx` as the branch `feature/x`, measured 2026-10-12). Refused
/// where it is empty or holds a control character -- nothing git or Gitea
/// would have issued. Escaping `/` and `.` sequences alike means the result
/// can never leave the route it is put in.
fn escaped(value: &str) -> Result<String, SourceError> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return Err(SourceError::protocol(format!(
            "gitea: {value:?} is not a usable branch or file name"
        )));
    }
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    if out == "." || out == ".." {
        return Err(SourceError::protocol(format!(
            "gitea: {value:?} is not a usable branch or file name"
        )));
    }
    Ok(out)
}
