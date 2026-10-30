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

    /// One page of fifty of `GET /repos/{o}/{r}/labels`, the repository's
    /// own labels, with Gitea's `X-Total-Count` where it sent one. A pull
    /// request's *Add label…* reads them all (#1194, #1235).
    pub(crate) async fn labels_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        page: u32,
    ) -> Result<(Vec<T>, Option<usize>), SourceError> {
        let path = format!("/repos/{}/{}/labels", segment(owner)?, segment(repo)?);
        self.label_page_as(&path, page).await
    }

    /// One page of fifty of `GET /orgs/{org}/labels`, the organisation's
    /// labels, which every repository it owns may use (#1235). Gitea answers
    /// 404 where `org` is a user, not an organisation.
    pub(crate) async fn org_labels_as<T: serde::de::DeserializeOwned>(
        &self,
        org: &str,
        page: u32,
    ) -> Result<(Vec<T>, Option<usize>), SourceError> {
        self.label_page_as(&format!("/orgs/{}/labels", segment(org)?), page)
            .await
    }

    /// Page `page` of the label listing at `path`, and its count.
    async fn label_page_as<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        page: u32,
    ) -> Result<(Vec<T>, Option<usize>), SourceError> {
        let (limit, page) = page_params(page);
        let (labels, total): (Option<Vec<T>>, _) =
            self.get_json_counted(path, &[limit, page]).await?;
        Ok((labels.unwrap_or_default(), total))
    }

    /// Whether `path` is a path in a tree this client would put in a route
    /// (no empty, `.` or `..` segment): asked before anything is sent for
    /// it, a tip included (#2241).
    pub(crate) fn tree_path(path: &str) -> Result<(), SourceError> {
        escaped_path(path).map(drop)
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

    /// `GET /repos/{o}/{r}/branch_protections/{rule}` -- one protection
    /// rule (#708). Gitea answers it to a repository administrator only; to
    /// anyone else it is a 403.
    pub(crate) async fn branch_protection_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        rule: &str,
    ) -> Result<T, SourceError> {
        self.get_json(
            &format!(
                "/repos/{}/{}/branch_protections/{}",
                segment(owner)?,
                segment(repo)?,
                escaped(rule)?
            ),
            &[],
        )
        .await
    }

    /// `DELETE /repos/{o}/{r}/branches/{branch}` (#707): the one delete this
    /// adapter makes, the name one escaped segment as for [`Self::branch_as`].
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the failure maps to, Gitea's sentence in it.
    pub(crate) async fn delete_branch(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
    ) -> Result<(), SourceError> {
        let path = format!(
            "{API_ROOT}/repos/{}/{}/branches/{}",
            segment(owner)?,
            segment(repo)?,
            escaped(branch)?
        );
        self.http
            .send(self.http.request(Method::DELETE, &path))
            .await
            .map(|_| ())
    }

    /// `GET /repos/{o}/{r}/commits/{ref}/status` (#710) -- the combined
    /// status: the latest status of each context reported on the commit
    /// `reference` names, a sha or a branch name (one escaped segment, as
    /// for [`Self::branch_as`]). A branch's view asks by its name so the read
    /// goes beside the one that learns its tip. A pull request's view
    /// asks by the head's sha, once, for *Checks* and the base's required
    /// checks alike (#721).
    pub(crate) async fn combined_status_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        reference: &str,
    ) -> Result<T, SourceError> {
        self.get_json(
            &format!(
                "/repos/{}/{}/commits/{}/status",
                segment(owner)?,
                segment(repo)?,
                escaped(reference)?
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

    /// The same compare's commits with their parents and no per-commit
    /// files (`files=false` is honoured there, measured on 1.27.3), for the
    /// two whole shas a compare's diff is between (#2310).
    pub(crate) async fn compare_commits<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        base: &str,
        head: &str,
    ) -> Result<T, SourceError> {
        self.get_json(
            &compare_path(owner, repo, base, head)?,
            &[
                ("files", "false".to_owned()),
                ("verification", "false".to_owned()),
            ],
        )
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
        let text = self
            .get_text_with(
                &compare_path(owner, repo, base, head)?,
                &[("output", "diff".to_owned())],
            )
            .await?;
        // Forgejo (17.0, #1195) ignores `output=diff` and answers the compare
        // as JSON. Parsed as a diff that is no files at all: *no changes*
        // drawn over a branch that has some. A diff never starts with `{`.
        if !text.trim_start().starts_with('{') {
            return Ok(text);
        }
        // A compare with no commits and no files is the empty diff. Its web
        // route answers that case with the compare page in HTML, not an empty
        // body (measured on 17.0.0), so it is answered here.
        let compare: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        if compare["total_commits"].as_u64() == Some(0)
            && compare["files"].as_array().is_some_and(Vec::is_empty)
        {
            return Ok(String::new());
        }
        // Its web route serves the diff to a token for a public repository
        // (measured on 17.0.0); for a private one it answers 404 whatever the
        // credentials, so that case is a refusal that says why, not a diff.
        let path = format!(
            "/{}/{}/compare/{}...{}.diff",
            segment(owner)?,
            segment(repo)?,
            escaped(base)?,
            escaped(head)?
        );
        let request = self.http.request(Method::GET, &path);
        match self.http.send(request).await {
            Ok(response) => {
                let text = response.text().await.map_err(|error| {
                    SourceError::protocol(format!(
                        "gitea: {path} answered an unreadable body: {error}"
                    ))
                })?;
                // A web route can answer 200 with a page that is no diff: a
                // sign-in page reached by redirect (REQUIRE_SIGNIN_VIEW), or
                // the compare page itself. Read as a diff that is no files,
                // that is the same silent *no changes*.
                let head = text.trim_start();
                if head.is_empty() || head.starts_with("diff ") {
                    Ok(text)
                } else {
                    Err(SourceError::protocol(format!(
                        "gitea: {path} answered something that is not a diff \
                         (this server ignores output=diff on a compare, as Forgejo does)"
                    )))
                }
            }
            Err(error) if error.status() == Some(404) => Err(SourceError::protocol(
                "gitea: this server ignores output=diff on a compare (Forgejo) and serves \
                 the diff of a private repository's branch to no token",
            )),
            Err(error) => Err(error),
        }
    }

    /// How many commits `sha` has that `not` does not, by Gitea's own
    /// `X-Total-Count` on a one-commit page of `/commits?sha=…&not=…`
    /// (measured on 1.27.3, 2026-10-12: it answered 18 for the seeded branch
    /// the reverse compare listed 18 commits for). The compare API takes no
    /// `limit` and would list every one of them, which on a branch thousands
    /// of commits stale is the overview's whole cost. `None` when the server
    /// sent no readable count.
    pub(crate) async fn count_commits_not_in(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        not: &str,
    ) -> Result<Option<u64>, SourceError> {
        let path = format!("/repos/{}/{}/commits", segment(owner)?, segment(repo)?);
        let query = one_commit_not_in(sha, not);
        let (_, total): (Vec<serde::de::IgnoredAny>, _) =
            self.get_json_counted(&path, &query).await?;
        Ok(total.map(|n| n as u64))
    }

    /// Whether `sha` has any commit `not` does not: one commit of
    /// `/commits?sha=…&not=…` asked for, and whether one came (#707). Unlike
    /// [`Self::count_commits_not_in`] it needs no count header.
    pub(crate) async fn has_commits_not_in(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        not: &str,
    ) -> Result<bool, SourceError> {
        let path = format!("/repos/{}/{}/commits", segment(owner)?, segment(repo)?);
        let page: Option<Vec<serde::de::IgnoredAny>> =
            self.get_json(&path, &one_commit_not_in(sha, not)).await?;
        Ok(page.is_some_and(|commits| !commits.is_empty()))
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

    /// One page of `GET /repos/{o}/{r}/git/trees/{sha}?recursive=true`
    /// (#2311): `per_page` entries of the whole tree at a commit, every
    /// directory's entries below it, and `total_count`, all of them.
    pub(crate) async fn git_tree_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        sha: &str,
        page: u32,
        per_page: usize,
    ) -> Result<T, SourceError> {
        self.get_json(
            &format!(
                "/repos/{}/{}/git/trees/{}",
                segment(owner)?,
                segment(repo)?,
                super::object_id(sha)?
            ),
            &[
                ("recursive", "true".to_owned()),
                ("page", page.to_string()),
                ("per_page", per_page.to_string()),
            ],
        )
        .await
    }

    /// The newest `limit` commits of `branch`, the listing projections off,
    /// with Gitea's `X-Total-Count` beside them: how many
    /// commits the branch has in all (#2311, the last-commit strip's *N
    /// commits*). `None` where Gitea sent no count. An empty repository's
    /// 409 is no commits, as for [`Self::commits`].
    pub(crate) async fn latest_commits_counted<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        branch: &str,
        limit: u32,
    ) -> Result<(Vec<T>, Option<u64>), SourceError> {
        let path = format!("/repos/{}/{}/commits", segment(owner)?, segment(repo)?);
        let query = [
            ("sha", branch.to_owned()),
            ("stat", "false".to_owned()),
            ("verification", "false".to_owned()),
            ("files", "false".to_owned()),
            ("limit", limit.to_string()),
            ("page", "1".to_owned()),
        ];
        match self.get_json_counted::<Option<Vec<T>>>(&path, &query).await {
            Err(error) if error.status() == Some(409) => Ok((Vec::new(), Some(0))),
            Err(error) => Err(error),
            Ok((page, total)) => Ok((page.unwrap_or_default(), total.map(|n| n as u64))),
        }
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

    /// `GET /repos/{o}/{r}/contents/{dir}?ref={at}` -- one directory's
    /// listing at a ref; `dir` empty is the top one (#711), where a README
    /// is found too: Gitea 1.27 has no `/readme` route (it answers 404 *page
    /// not found*).
    pub(crate) async fn contents_at_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        dir: &str,
        at: &str,
    ) -> Result<Vec<T>, SourceError> {
        self.get_json(&contents_path(owner, repo, dir)?, &[("ref", at.to_owned())])
            .await
    }

    /// `GET /repos/{o}/{r}/contents-ext/{file}?ref={at}` -- one file's own
    /// entry at a ref without its bytes (its size and blob sha, among
    /// others): read beside every file's [`Self::raw_capped`] for the blob an
    /// edit is made against (#883), and its size where a cut answer did not
    /// say how long it is.
    ///
    /// `contents-ext` rather than `contents`, which answers the whole file
    /// as base64 up to Gitea's 10 MiB blob limit and walks the history for
    /// the file's last commit: a 10 MB file came back as 13.5 MB of JSON
    /// beside a raw read capped at 1 MB (1.27.3, measured 2026-10-16), and
    /// `contents-ext` answered the same entry in 784 bytes. A Gitea from
    /// before `contents-ext` (1.24) answers its route 404, and the file's
    /// [`Self::contents_entry_as`] is read instead -- from then on straight
    /// away, once `contents` found the file the 404 was for (#2241).
    pub(crate) async fn file_entry_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        file: &str,
        at: &str,
    ) -> Result<T, SourceError> {
        #[derive(serde::Deserialize)]
        struct Ext<T> {
            file_contents: Option<T>,
        }
        use std::sync::atomic::Ordering;
        if self.no_contents_ext.load(Ordering::Relaxed) {
            return self.contents_entry_as(owner, repo, file, at).await;
        }
        let route = format!(
            "/repos/{}/{}/contents-ext/{}",
            segment(owner)?,
            segment(repo)?,
            escaped_path(file)?
        );
        match self
            .get_json::<Ext<T>>(&route, &[("ref", at.to_owned())])
            .await
        {
            Ok(Ext {
                file_contents: Some(entry),
            }) => Ok(entry),
            Ok(Ext {
                file_contents: None,
            }) => Err(SourceError::protocol(format!(
                "gitea: {file:?} at {at:?} is not a file"
            ))),
            Err(error) if error.status() == Some(404) => {
                let entry = self.contents_entry_as(owner, repo, file, at).await;
                // Only where `contents` found the file: then it was the
                // route that was missing, not the file.
                if entry.is_ok() {
                    self.no_contents_ext.store(true, Ordering::Relaxed);
                }
                entry
            }
            Err(error) => Err(error),
        }
    }

    /// `GET /repos/{o}/{r}/contents/{file}?ref={at}` -- one file's own entry
    /// at a ref, its bytes as base64 among it: what a commit's re-read
    /// compares (#822), and [`Self::file_entry_as`] on a Gitea without
    /// `contents-ext`.
    pub(crate) async fn contents_entry_as<T: serde::de::DeserializeOwned>(
        &self,
        owner: &str,
        repo: &str,
        file: &str,
        at: &str,
    ) -> Result<T, SourceError> {
        self.get_json(
            &contents_path(owner, repo, file)?,
            &[("ref", at.to_owned())],
        )
        .await
    }

    /// `PUT /repos/{o}/{r}/contents/{file}` with `body` as JSON (#822): one
    /// file's new version committed, the commit Gitea made in the answer.
    /// `file` is escaped as the reads' [`Self::contents_entry_as`] is.
    ///
    /// # Errors
    ///
    /// The [`SourceError`] the failure maps to, Gitea's sentence in it.
    pub(crate) async fn put_contents(
        &self,
        owner: &str,
        repo: &str,
        file: &str,
        body: &serde_json::Value,
    ) -> Result<knobas_http::Response, SourceError> {
        let path = format!("{API_ROOT}{}", contents_path(owner, repo, file)?);
        self.http
            .send(self.http.request(Method::PUT, &path).json(body))
            .await
    }

    /// `GET /repos/{o}/{r}/raw/{file}?ref={at}` -- one file's bytes at a ref,
    /// read no further than `cap` bytes past which it is cut (#711): a file
    /// many megabytes long costs what is drawn of it, not its whole size.
    pub(crate) async fn raw_capped(
        &self,
        owner: &str,
        repo: &str,
        file: &str,
        at: &str,
        cap: usize,
    ) -> Result<RawFile, SourceError> {
        self.raw_read(owner, repo, file, at, cap, false).await
    }

    /// The same read for a file wanted whole or not at all (#2256): an
    /// answer whose length is already over `cap` is cut before any of its
    /// body is read, so a refused file costs its headers.
    pub(crate) async fn raw_whole(
        &self,
        owner: &str,
        repo: &str,
        file: &str,
        at: &str,
        cap: usize,
    ) -> Result<RawFile, SourceError> {
        self.raw_read(owner, repo, file, at, cap, true).await
    }

    async fn raw_read(
        &self,
        owner: &str,
        repo: &str,
        file: &str,
        at: &str,
        cap: usize,
        whole: bool,
    ) -> Result<RawFile, SourceError> {
        let path = format!(
            "{API_ROOT}/repos/{}/{}/raw/{}",
            segment(owner)?,
            segment(repo)?,
            escaped_path(file)?
        );
        let request = self
            .http
            .request(Method::GET, &path)
            .query(&[("ref", at.to_owned())]);
        let mut response = self.http.send(request).await?;
        let length = response.content_length();
        if whole && length.is_some_and(|length| length > cap as u64) {
            return Ok(RawFile {
                bytes: Vec::new(),
                cut: true,
                size: length,
            });
        }
        let mut bytes = Vec::new();
        let mut cut = false;
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            SourceError::protocol(format!(
                "gitea: {path} answered an unreadable body: {error}"
            ))
        })? {
            bytes.extend_from_slice(&chunk);
            if bytes.len() > cap {
                cut = true;
                break;
            }
        }
        // Only where the body is not compressed: `reqwest` undoes a gzip
        // answer and then has no length to give, so a cut read without one
        // leaves the size to its caller.
        let size = if cut {
            length
        } else {
            Some(bytes.len() as u64)
        };
        Ok(RawFile { bytes, cut, size })
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

/// One commit of what `sha` has that `not` has not, with every projection
/// Gitea defaults on turned off: the query both of those reads send.
fn one_commit_not_in(sha: &str, not: &str) -> [(&'static str, String); 6] {
    [
        ("sha", sha.to_owned()),
        ("not", not.to_owned()),
        ("stat", "false".to_owned()),
        ("verification", "false".to_owned()),
        ("files", "false".to_owned()),
        ("limit", "1".to_owned()),
    ]
}

/// A file's bytes as [`GiteaClient::raw_capped`] read them.
#[derive(Debug)]
pub(crate) struct RawFile {
    /// What was read: the whole file, or, where `cut`, more than the cap.
    pub(crate) bytes: Vec<u8>,
    /// The file is longer than the cap and was not read to its end.
    pub(crate) cut: bool,
    /// The whole file's size: what was read, or where it was cut, what Gitea
    /// said its length was; `None` for a cut read whose answer did not say.
    pub(crate) size: Option<u64>,
}

/// `/repos/{o}/{r}/contents[/{path}]`, `path` empty for the top directory.
fn contents_path(owner: &str, repo: &str, path: &str) -> Result<String, SourceError> {
    let mut route = format!("/repos/{}/{}/contents", segment(owner)?, segment(repo)?);
    if !path.is_empty() {
        route.push('/');
        route.push_str(&escaped_path(path)?);
    }
    Ok(route)
}

/// A path in a tree as path segments, each [`escaped`]: `src/lib` stays two
/// segments, and an empty, `.` or `..` segment is refused, so the path can
/// never leave the route it is put in.
fn escaped_path(value: &str) -> Result<String, SourceError> {
    let segments: Vec<String> = value.split('/').map(escaped).collect::<Result<_, _>>()?;
    Ok(segments.join("/"))
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
