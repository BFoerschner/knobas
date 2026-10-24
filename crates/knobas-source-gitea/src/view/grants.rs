//! What this source's credential may write in one repository, asked of Gitea
//! itself after a view is drawn (issue #2030, spec #1192 story 10).
//!
//! The descriptor's `write_ops` say what the adapter can do, never what the
//! token it was given may: a token minted with `read:repository` reads every
//! view and is refused every write with a 403, after the write was queued.
//! So a view that offers writes asks, and draws the ones the token cannot
//! make disabled with the reason ([`SourceView::refuse_writes`]). A token can
//! be replaced, so the writes stay drawn (ADR-0020 rule 4).
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
//! The branch probe says one more thing. Gitea checks that the **account**
//! may push before it reads the body, and refuses one that may not with a
//! 403 (`user should have a permission to write to a repo`, measured with a
//! reader of a public repository). The sentence is not matched: any 403 on
//! the branch probe other than the scope refusal is read as this one, the
//! only other check Gitea makes there before the body. Only the writes that push need that permission (a
//! reader may still comment and review), so only those are disabled for it.
//!
//! Anything else the probe meets -- a 401, a 5xx, a timeout, an archived
//! repository's 423 -- is no verdict, and the writes stay as drawn: the probe
//! narrows what is offered and never fails a view.

use knobas_source::SourceError;
use knobas_source::view::SourceView;
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
/// `/repos/{o}/{r}/issues` are `issue`, every other route a view's write
/// takes is `repository` (the table in [`crate::write`]'s docs). A pull
/// request's comment and its labels go through the issue of the same index.
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

/// What one probe learned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Refusal {
    /// The token lacks the category's write scope.
    Scope,
    /// The account may not push here (the branch probe only).
    Account,
}

/// Why a write the credential cannot make is drawn disabled.
fn reason(category: Category, refusal: Refusal) -> String {
    match refusal {
        Refusal::Scope => format!(
            "This token cannot write: it lacks the {} scope",
            category.scope()
        ),
        Refusal::Account => "This account cannot push to this repository".to_owned(),
    }
}

/// Ask Gitea what the token may write in `owner/repo`, for the categories the
/// view's writes fall in, and draw the refused writes disabled.
pub(super) async fn narrow(source: &GiteaSource, owner: &str, repo: &str, view: &mut SourceView) {
    let mut offered = Vec::new();
    // Read the ops through the same walk that disables them, so a write in a
    // block this module never heard of is still asked about. Refusing none
    // changes nothing.
    view.refuse_writes(|op| {
        offered.push(op.to_owned());
        None
    });
    let wants = |category| offered.iter().any(|op| self::category(op) == category);
    let ask = |category| async move {
        if wants(category) {
            probe(source, owner, repo, category).await
        } else {
            None
        }
    };
    let (repository, issue) = tokio::join!(ask(Category::Repository), ask(Category::Issue));
    if repository.is_none() && issue.is_none() {
        return;
    }
    view.refuse_writes(|op| {
        let category = category(op);
        let refusal = match category {
            Category::Repository => repository,
            Category::Issue => issue,
        };
        match refusal? {
            Refusal::Scope => Some(reason(category, Refusal::Scope)),
            Refusal::Account => pushes(op).then(|| reason(category, Refusal::Account)),
        }
    });
}

/// One probe: the category's write with an empty body (the module docs).
async fn probe(
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

/// What a probe's failure says. `knobas-http` reads Gitea's scope sentence as
/// `Unauthorized` with its 403 (`client::forbidden_refusal`, #722) and any
/// other 403 as a refusal carrying Gitea's sentence.
fn refusal_of(category: Category, failure: Option<&SourceError>) -> Option<Refusal> {
    match failure? {
        SourceError::Unauthorized { status: Some(403) } => Some(Refusal::Scope),
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
