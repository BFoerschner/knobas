//! What a view's credential may write, and what it says where the credential
//! may not read a part of it ([`unread`], #2042).
//!
//! A view that offers writes asks Gitea, once it knows which writes it
//! offers, what the token may make of them ([`narrow`], #2030), and draws the
//! ones it cannot make disabled with the reason
//! ([`SourceView::refuse_writes`]). How Gitea is asked, and why the probe
//! changes nothing, is [`crate::grants`]': the probe a shell surface's
//! `Source::refused_writes` sends too (#2054).

use knobas_source::SourceError;
use knobas_source::view::SourceView;

use crate::grants::is_scope_refusal;
use crate::source::GiteaSource;

/// The read scope Gitea checks a read under `/repos/{o}/{r}/issues` or
/// `/repos/{o}/{r}/labels` against: a pull request's discussion and its
/// repository's labels (#2042).
pub(super) const READ_ISSUE: &str = "read:issue";

/// The read scope Gitea checks `/orgs/{org}/…` against: an organisation's
/// labels.
pub(super) const READ_ORGANIZATION: &str = "read:organization";

/// Why a view cannot draw `what`, where Gitea refused its read for the
/// token's scope (`scope` being the one the route asks for); `None` for any
/// other failure. Gitea checks a read's scope by its route's category as it
/// checks a write's ([`crate::grants`]), and measured on Gitea 1.27.3 a token
/// minted with `read:repository` alone is refused a pull request's comments
/// and every label listing with that 403, and served everything else a view
/// reads (#2042). A view draws what it could read and says this where the
/// rest would be.
pub(super) fn unread(error: &SourceError, what: &str, scope: &str) -> Option<String> {
    is_scope_refusal(error)
        .then(|| format!("This token cannot read {what}: it lacks the {scope} scope"))
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
    let ops: Vec<&str> = offered.iter().map(String::as_str).collect();
    let grants = crate::grants::probe(source, owner, repo, &ops).await;
    if grants.none() {
        return;
    }
    view.refuse_writes(|op| grants.refusal(op));
}
