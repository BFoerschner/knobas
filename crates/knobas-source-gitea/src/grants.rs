//! What this source's credential may write in one repository, asked of Gitea
//! itself (issue #2030, spec #1192 story 10): by a view after it is drawn
//! (`view::grants::narrow`), and by [`Source::refused_writes`] for the writes
//! a shell surface draws outside a view (#2054: a ticket's comment and
//! transition, *Create ticket*).
//!
//! The descriptor's `write_ops` say what the adapter can do, never what the
//! token it was given may: a token minted with `read:repository` reads every
//! view and is refused every write with a 403, after the write was queued.
//! So a surface that offers writes asks, and draws the ones the token cannot
//! make disabled with the reason. A token can be replaced, so the writes stay
//! drawn (ADR-0020 rule 4).
//!
//! **Gitea has no endpoint that answers a token's scopes** to the token: its
//! token listing takes basic auth. What it has is the check every write
//! passes first. A token's scope is checked per category of the route
//! (`repository`, `issue`) before anything else the route does, so a write
//! sent with an empty body is refused for the scope where the token lacks it,
//! and otherwise fails the body's validation with a 422 and changes nothing.
//! Measured on Gitea 1.27.3, 2026-10-24, and witnessed for both forges by
//! `tests/live_gitea.rs`:
//!
//! | probe, body `{}` | token without the scope | token with it |
//! |---|---|---|
//! | `POST /repos/{o}/{r}/branches` | 403 `token does not have at least one of required scope(s), required=[write:repository], …` | 422 `[BranchName]: Required` |
//! | `POST /repos/{o}/{r}/issues` | 403 `… required=[write:issue], …` | 422 `[Title]: Required` |
//!
//! **Why `{}` creates nothing**, read in the forges' source (Gitea 1.19,
//! 1.21, 1.24 and `main`, Forgejo's `forgejo` branch, 2026-10-24): in
//! `routers/api/v1/api.go` both routes bind the body
//! (`bind(api.CreateBranchRepoOption{})`, `bind(api.CreateIssueOption{})`)
//! ahead of `repo.CreateBranch` and `repo.CreateIssue`, and `bind` answers
//! 422 and stops the chain when validation fails, before the handler runs.
//! `new_branch_name` and `title` are `binding:"Required"` in
//! `modules/structs` on every one of them. No webhook, notification or audit
//! entry is made before the handler. Before 1.19 a token has no scopes, so
//! the probe reads 422 or the account's 403.
//! `live_gitea` counts both repositories' branches, issues and pull requests
//! around its probes.
//!
//! The branch probe says one more thing. Gitea checks that the **account**
//! may push before it reads the body, and refuses one that may not with a
//! 403 (`user should have a permission to write to a repo`, measured with a
//! reader of a public repository). The sentence is not matched: any 403 on
//! the branch probe other than the scope refusal is read as this one, the
//! only other check Gitea makes there before the body. Only the writes that
//! push need that permission (a reader may still comment and review), so
//! only those are disabled for it.
//!
//! Anything else the probe meets -- a 401, a 5xx, a timeout, an archived
//! repository's 423 -- is no verdict, and the writes stay as drawn: the probe
//! narrows what is offered and never fails a view or a surface.
//!
//! [`Source::refused_writes`]: knobas_source::Source::refused_writes

use knobas_source::SourceError;
use knobas_source::grants::{RefusedWrite, WriteAsk};
use serde_json::json;

use crate::source::GiteaSource;

/// The two scope categories Gitea checks the writes this adapter makes
/// against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Category {
    Repository,
    Issue,
}

impl Category {
    /// The scope a token needs to write in this category.
    fn scope(self) -> &'static str {
        match self {
            Self::Repository => "write:repository",
            Self::Issue => "write:issue",
        }
    }

    /// The write whose empty body asks for this category's scope.
    fn probe_path(self, owner: &str, repo: &str) -> String {
        match self {
            Self::Repository => format!("/repos/{owner}/{repo}/branches"),
            Self::Issue => format!("/repos/{owner}/{repo}/issues"),
        }
    }
}

/// The category Gitea checks `op`'s request against: the routes under
/// `/repos/{o}/{r}/issues` are `issue`, every other route a write takes is
/// `repository` (the table in [`crate::write`]'s docs). A pull request's
/// comment and its labels go through the issue of the same index, and a
/// ticket is created under `/issues`.
fn category(op: &str) -> Category {
    match op {
        crate::WRITE_OP_COMMENT
        | crate::WRITE_OP_TRANSITION
        | crate::WRITE_OP_CREATE_TICKET
        // *Add label…*, the one source action this adapter declares.
        | knobas_source::SOURCE_ACTION => Category::Issue,
        _ => Category::Repository,
    }
}

/// The writes that push to the repository, which Gitea refuses an account
/// without write access to its code.
fn pushes(op: &str) -> bool {
    matches!(
        op,
        crate::WRITE_OP_CREATE_BRANCH
            | crate::WRITE_OP_DELETE_BRANCH
            | crate::WRITE_OP_COMMIT_FILE
            | crate::WRITE_OP_MERGE_PULL_REQUEST
    )
}

/// Gitea's refusal of a request for the token's scope: `knobas-http` reads
/// Gitea's scope sentence as `Unauthorized` with its 403
/// (`client::forbidden_refusal`, #722). A 403 whose body is not Gitea's
/// envelope (a proxy's page) reads the same way, since what it refuses is
/// unknown and the token is the conservative guess, so it is taken for the
/// scope refusal here too; any 403 carrying another Gitea sentence is not.
pub(crate) fn is_scope_refusal(error: &SourceError) -> bool {
    matches!(error, SourceError::Unauthorized { status: Some(403) })
}

/// What one probe learned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    /// The token lacks the category's write scope.
    Scope,
    /// The account may not push here (the branch probe only).
    Account,
}

/// What the probes learned about one repository: a refusal per category, or
/// none where the category was not asked or the probe gave no verdict.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Grants {
    repository: Option<Refusal>,
    issue: Option<Refusal>,
}

impl Grants {
    /// Whether nothing was refused.
    pub(crate) fn refuses_nothing(&self) -> bool {
        self.repository.is_none() && self.issue.is_none()
    }

    /// Why the credential cannot make `op` in this repository, or `None`.
    pub(crate) fn refusal(&self, op: &str) -> Option<String> {
        let category = category(op);
        let refusal = match category {
            Category::Repository => self.repository,
            Category::Issue => self.issue,
        };
        match refusal? {
            Refusal::Scope => Some(format!(
                "This token cannot write: it lacks the {} scope",
                category.scope()
            )),
            Refusal::Account => {
                pushes(op).then(|| "This account cannot push to this repository".to_owned())
            }
        }
    }
}

/// Ask Gitea what the token may write in `owner/repo`, probing only the
/// categories `ops` fall in, side by side.
pub(crate) async fn probe(source: &GiteaSource, owner: &str, repo: &str, ops: &[&str]) -> Grants {
    let wants = |category| ops.iter().any(|op| self::category(op) == category);
    let ask = |category| async move {
        if wants(category) {
            probe_one(source, owner, repo, category).await
        } else {
            None
        }
    };
    let (repository, issue) = tokio::join!(ask(Category::Repository), ask(Category::Issue));
    Grants { repository, issue }
}

/// The asks among `asks` the token cannot make (#2054): each repository the
/// asks name is probed once, for the categories its asks fall in. An ask of another source, an id this adapter
/// did not issue and an op it does not declare are not answered.
pub(crate) async fn refused(source: &GiteaSource, asks: &[WriteAsk]) -> Vec<RefusedWrite> {
    let declared = crate::descriptor_template().write_ops;
    let mut by_repository: Vec<((String, String), Vec<&WriteAsk>)> = Vec::new();
    for ask in asks {
        if !declared.contains(&ask.op) {
            continue;
        }
        let Ok(key) = source.key_of(&ask.entity) else {
            continue;
        };
        let (owner, repo) = key.repository();
        let repository = (owner.to_owned(), repo.to_owned());
        match by_repository.iter_mut().find(|(r, _)| *r == repository) {
            Some((_, repo_asks)) => repo_asks.push(ask),
            None => by_repository.push((repository, vec![ask])),
        }
    }
    // One repository after another: a surface asks about one entity, or one
    // picked project, so there is rarely a second to wait on.
    let mut answered: Vec<RefusedWrite> = Vec::new();
    for ((owner, repo), repo_asks) in &by_repository {
        let ops: Vec<&str> = repo_asks.iter().map(|a| a.op.as_str()).collect();
        let grants = probe(source, owner, repo, &ops).await;
        answered.extend(repo_asks.iter().filter_map(|ask| {
            grants
                .refusal(&ask.op)
                .map(|why| RefusedWrite::of(ask, why))
        }));
    }
    asks.iter()
        .filter_map(|ask| answered.iter().find(|r| r.refuses(ask)).cloned())
        .collect()
}

/// One probe: the category's write with an empty body (the module docs).
async fn probe_one(
    source: &GiteaSource,
    owner: &str,
    repo: &str,
    category: Category,
) -> Option<Refusal> {
    let owner = crate::client::GiteaClient::path_segment(owner).ok()?;
    let repo = crate::client::GiteaClient::path_segment(repo).ok()?;
    let answer = source
        .client
        .post_once(&category.probe_path(owner, repo), &json!({}))
        .await;
    refusal_of(category, answer.err().as_ref())
}

/// What a probe's failure says: the scope refusal ([`is_scope_refusal`]), or
/// any other 403, which `knobas-http` reads as a refusal carrying Gitea's
/// sentence.
fn refusal_of(category: Category, failure: Option<&SourceError>) -> Option<Refusal> {
    match failure? {
        scope if is_scope_refusal(scope) => Some(Refusal::Scope),
        other if other.status() == Some(403) && category == Category::Repository => {
            Some(Refusal::Account)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_answer_reads_as_the_measured_table_says() {
        let scope = SourceError::Unauthorized { status: Some(403) };
        let account = SourceError::Protocol {
            status: Some(403),
            message: "HTTP 403 Forbidden: user should have a permission to write to a repo".into(),
        };
        let invalid = SourceError::Protocol {
            status: Some(422),
            message: "HTTP 422 Unprocessable Entity: [BranchName]: Required".into(),
        };
        for category in [Category::Repository, Category::Issue] {
            assert_eq!(refusal_of(category, Some(&scope)), Some(Refusal::Scope));
            assert_eq!(refusal_of(category, Some(&invalid)), None);
            assert_eq!(refusal_of(category, None), None);
            assert_eq!(
                refusal_of(category, Some(&SourceError::unauthorized())),
                None
            );
        }
        assert_eq!(
            refusal_of(Category::Repository, Some(&account)),
            Some(Refusal::Account)
        );
        // Unmeasured on the issue probe, so no verdict.
        assert_eq!(refusal_of(Category::Issue, Some(&account)), None);
    }
}
