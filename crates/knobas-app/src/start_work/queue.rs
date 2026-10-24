//! The real [`Steps`]: the write queue, the link store and the mirror.
//!
//! Everything here is IO, and deliberately nothing else -- the orchestrator
//! next door holds the policy, and this is what it holds policy *over*. That
//! split is what lets a fake stand in for this file and the flow's real
//! behaviour still be the thing under test.
//!
//! It does not name `WriteOp`: `dispatch` forwards the payload the plan
//! composed to `crate::sources::write_queue::submit`, which is where a write op
//! is decoded and checked against what the source declares.

use std::borrow::Borrow;

use async_trait::async_trait;
use knobas_core::entity::EntityRef;
use knobas_core::payload::Declarations;
use knobas_core::write_queue::{self, QueuedWrite, WriteState};
use sqlx::PgPool;

use super::{Landing, Linked, PullRequestOnHead, Steps};
use crate::sources::SourcesState;
use crate::{IpcError, IpcErrorCode};

/// The mirrored kinds the flow reads back.
///
/// Generic kinds, from `CONTEXT.md`'s list -- not one adapter's vocabulary.
/// Every adapter that mirrors a repository's refs declares them under these
/// names, which is what lets these two reads be written once.
const KIND_BRANCH: &str = "branch";
pub(crate) const KIND_PR: &str = "pr";

/// The flow's window onto the world.
///
/// Over a borrowed state (`Queue { state: &state }`) for a flow its command
/// waits on, or an owned copy (`Queue { state: state.clone() }`) for a run
/// behind its command's answer, which outlives the command (#779). One
/// implementation of [`Steps`] serves both (#789).
pub struct Queue<S> {
    pub state: S,
}

#[async_trait]
impl<S: Borrow<SourcesState> + Send + Sync> Steps for Queue<S> {
    async fn dispatch(&self, payload: &serde_json::Value) -> Result<(i64, Landing), IpcError> {
        // The same call the *Comment* button makes. A start-work step is not a
        // privileged write: it queues, it can be held, it can be refused, and
        // it shows up in the pending-writes panel like anything else.
        let queued =
            crate::sources::write_queue::submit(self.state.borrow(), payload.clone()).await?;
        // The row came back **as queued**, before the attempt -- which is the
        // honest shape for that call and says nothing about the outcome. Read
        // it again for what actually happened.
        let landing = self.landing_of(queued.id).await?;
        Ok((queued.id, landing))
    }

    async fn landing_of(&self, write_id: i64) -> Result<Landing, IpcError> {
        let row = write_queue::get(&self.state.borrow().pool, write_id).await?;
        Ok(landing_from(row))
    }

    async fn link(&self, from: &str, to: &str, relation: &str) -> Result<Linked, IpcError> {
        match crate::commands::entity::create_link_inner(
            &self.state.borrow().pool,
            from,
            to,
            Some(relation),
            None,
        )
        .await
        {
            Ok(_) => Ok(Linked::Made),
            // The pair already carries this relation. For a step whose job is
            // that the relationship *exists*, that is the outcome, not a
            // failure -- and it is what makes this step idempotent under retry
            // without a read of its own.
            Err(error) if error.code == IpcErrorCode::Conflict => Ok(Linked::Already),
            Err(error) => Err(error),
        }
    }

    async fn branch(&self, repo: &EntityRef, name: &str) -> Result<Option<String>, IpcError> {
        branch_named(&self.state.borrow().pool, repo, name).await
    }

    async fn pull_request(
        &self,
        repo: &EntityRef,
        head: &str,
        base: &str,
    ) -> Result<PullRequestOnHead, IpcError> {
        // The declaration is resolved here rather than threaded through
        // `Steps`: it is a property of the running binary's registry, which
        // this implementation has and the trait's other implementations --
        // the fake in the seam tests -- have no use for. `merge::follow_merges`
        // takes it as an argument instead because it is one pass over the whole
        // corpus and the caller already holds it; this is two or three lookups
        // per flow -- the create step's, and `link_step`'s, twice where it
        // asks for a refresh -- and the listing behind `declared_paths` is the
        // same round trip the inbox makes per read.
        let state: &SourcesState = self.state.borrow();
        let declarations =
            crate::sources::paths::declared_paths(&state.pool, state.registry.as_ref()).await?;
        pull_request_on_head(&state.pool, &declarations, repo, head, base).await
    }

    async fn pull_request_named(
        &self,
        write_id: i64,
    ) -> Result<crate::sources::write_queue::Named, IpcError> {
        crate::sources::write_queue::entity_named(&self.state.borrow().pool, write_id, KIND_PR)
            .await
    }

    async fn refresh(&self, source: &str) {
        // Wait for the run, rather than triggering and hoping: the very next
        // thing the caller does is look for the pull request this run is
        // fetching. One implementation of that wait, in
        // `crate::sources::write_queue::sync_after_write`, because #289's
        // protocol publish needs the identical thing for the identical reason --
        // what a create made has an entity only once a sync has seen it, and
        // reading the mirror before the run has finished is reading it too
        // early. **Or before a run that could have seen it has
        // finished** -- waiting out the one this flow's own branch write left
        // in flight is `Scheduler::sync_after_write`'s job and was the #358
        // flake.
        crate::sources::write_queue::sync_after_write(self.state.borrow(), source).await;
    }
}

/// A queue row, or its absence, read as a step's [`Landing`].
///
/// A held row says which of the three holds it is (CONTEXT.md, *Held write*:
/// the reasons are never collapsed). A send never answered comes first
/// ([`QueuedWrite::may_have_landed`]), turned off or not: whether the write may
/// already be at the source is what decides that nothing may invite a second
/// send (#1973). Then a source the user turned off, then the drift.
#[must_use]
pub fn landing_from(row: Option<QueuedWrite>) -> Landing {
    let Some(row) = row else {
        return Landing::Withdrawn;
    };
    match row.state {
        WriteState::Sent => Landing::Sent,
        WriteState::Pending => Landing::Waiting { detail: row.detail },
        WriteState::Held if let Some(unanswered) = row.unconfirmed_send() => Landing::Unconfirmed {
            source: unanswered.source.unwrap_or(&row.source_id).to_owned(),
            detail: unanswered
                .detail
                .filter(|detail| !detail.is_empty())
                .map(str::to_owned),
        },
        WriteState::Held if !row.source_enabled => Landing::SourceOff,
        WriteState::Held => Landing::Held { detail: row.detail },
        WriteState::Refused => Landing::Refused {
            detail: row
                .detail
                .unwrap_or_else(|| "the source refused the write".to_owned()),
        },
        WriteState::Discarded => Landing::Withdrawn,
    }
}

/// The branch of a repository whose own name is `$3`.
///
/// `title` is the branch's name: contract §4.1 makes every adapter build it
/// from the item, and `map.rs` puts the bare branch name there. Reading it
/// rather than reconstructing the entity id is what keeps this statement out of
/// the business of how a branch's key is spelled -- `owner/repo@refs/heads/x`
/// for Gitea, something else for the next adapter.
const BRANCH_BY_NAME: &str = "select entity_id from sync.live_item
      where entity_id like $1 and kind = $2 and title = $3
      order by entity_id limit 1";

/// The mirrored branch of `repo` named `name`, or `None`: the look-up behind
/// [`Queue::branch`], and behind a new branch's link once it has arrived
/// (#984, `sources::write_queue::link_on_arrival`).
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn branch_named(
    pool: &PgPool,
    repo: &EntityRef,
    name: &str,
) -> Result<Option<String>, IpcError> {
    // Narrowed to the repository by an id prefix before the kind filter:
    // `sync.item` has no index on either and the prefix is the selective
    // half.
    found(pool, BRANCH_BY_NAME, &like_prefix(repo), KIND_BRANCH, name).await
}

/// Every pull request of a repository opened from head branch `$3` into base
/// branch `$5`, each with what its own record says about whether it is still
/// open.
///
/// # A pull request is the pair, head and base (#1952)
///
/// That is the forge's own identity for a duplicate: Gitea refuses a second
/// open pull request for the same head *and* base ("pull request already
/// exists for these targets") and opens one from the same head into another
/// base without complaint. Matched on the head alone, a colleague's open pull
/// request from the flow's head into `release/x` settled the create step
/// "already open", so the flow's own into `main` was never opened, and the
/// link then went to the colleague's. Both halves sit at the literal path
/// Gitea mirrors (`head.ref`, `base.ref`), the pair it declares as
/// `KindPaths::head` and `KindPaths::base` (#1227). A record whose base is
/// absent or not that string matches nothing: the create step then dispatches
/// and the forge refuses a duplicate in its own words, and the link step finds
/// nothing and settles failed -- the direction [`super::PullRequestOnHead`]
/// pins.
///
/// **A source-shaped read outside an adapter, and it is here because there is
/// no other way.** A pull request open from the head into the base is what the
/// create step looks for before it writes, and what a create whose receipt named nothing
/// is found by afterwards (a receipt that names one is resolved by its id,
/// `sources::write_queue::entity_named`, #1949); §4.1 guarantees `title`,
/// `body_text`, `updated_at` and a verbatim `payload`, and neither the head
/// and base branches nor the state live anywhere but the last of those. That is the same
/// seam #43 recorded for a ticket's status, met here in the read direction. It
/// is confined to this one statement deliberately: a source that spells its
/// pull requests differently needs one more `coalesce` here and nothing else
/// anywhere.
///
/// # Why it is a list and no longer `limit 1` (#359)
///
/// A head branch is re-usable, and the mirror deliberately holds finished pull
/// requests -- `state=all`, with the reason written at
/// `knobas_source_gitea::client::Client::pulls`. So `order by entity_id limit
/// 1` picked **lexicographically among every pull request that head ever
/// had**, merged ones included: on a head whose pull requests are `#7`
/// (merged) and `#8` (open) it answers `#7`, which settled the create step
/// "already open" against a merge and then drew the ticket's link to it.
///
/// # The two facts, and which of them decides
///
/// `state` is the one the flow asks about and the one [`pick_open`] decides
/// on; the declared merged flag can only veto. [`super::PullRequestOnHead`]
/// argues that at length -- the short of it is that a pull request closed
/// *without* merging carries `merged: false`, so a rule built on the
/// declaration alone calls it open and commits the same bug one state over.
///
/// `state` is read at a literal path because no [`KindPaths`] slot answers
/// "is this open" and pointing `status_name` at it would change what every
/// reader of a status renders for a pull request. The **`open` spelling is
/// [`STATE_OPEN`]**, the coalesce point ADR-0007's requirement 2 promises;
/// both adapters that mirror pull requests today spell it that way.
///
/// The `jsonb_typeof(...) = 'string'` guard is the same triple refusal
/// `declared_string!` makes: a payload is a verbatim source record, and `->>`
/// on an object would answer that object's JSON text, which is neither `open`
/// nor a miss. It has to be a string, and trimming it has to leave something.
///
/// The merged flag is read at the path the source **declares** (#277, `$4`),
/// which is the same declaration `super::merge` follows, so the two halves of
/// the pull-request story cannot disagree about what merged means.
///
/// `order by i.entity_id` survives as the tie-break among rows the caller
/// cannot otherwise tell apart, so two open rows on one head and base -- which
/// the forge refuses to make, but a mirror is not the forge -- answer the same
/// one on every call rather than whichever the planner reached first.
///
/// [`KindPaths`]: knobas_core::payload::KindPaths
const PULL_REQUEST_BY_HEAD_AND_BASE: &str = concat!(
    "select i.entity_id, ",
    knobas_core::declared_flag!("$4", "merged"),
    " as merged,
        case when jsonb_typeof(i.payload->'state') = 'string'
             then nullif(btrim(i.payload->>'state'), '') end as state
       from sync.live_item i
      where i.entity_id like $1 and i.kind = $2
        and i.payload->'head'->>'ref' = $3
        and i.payload->'base'->>'ref' = $5
      order by i.entity_id"
);

/// What a source calls a pull request that is still open.
///
/// The one spelling knobas holds, and the coalesce point ADR-0007's
/// requirement 2 promises: a forge that says `OPENED` is a change to this
/// item and to [`is_open`], and to nothing else anywhere.
///
/// **Gitea is the only source this read reaches today**, and it mirrors its
/// `state` verbatim: `open` or `closed`, exactly the two words, so both
/// classifications below are literal for it. `knobas_source_mock` spells its
/// own state `open | merged` but never gets here at all --
/// [`PULL_REQUEST_BY_HEAD_AND_BASE`] finds a record by `payload->'head'->>'ref'` and
/// the mock's pull request carries `from`/`to` and no `head` object. That is
/// worth knowing in both directions: it is why the mock is an argument about
/// *shape* below and not a corpus, and it is why the word
/// [`is_finished`] picks is right for every record that actually arrives.
const STATE_OPEN: &str = "open";

/// The repository's id as a `like` prefix, with `like`'s own metacharacters
/// escaped.
///
/// `_` is common in repository names and matches *any* character in a
/// pattern, so an unescaped `gitea:acme/payout_service%` would also match
/// `payoutXservice` -- and a look-before-write that found the wrong
/// repository's branch would settle a step "succeeded" against an effect that
/// does not exist. Postgres' default escape character is `\`, and the value
/// is bound, so escaping the three metacharacters is the whole job.
///
/// The escaping itself is [`escape_like`].
fn like_prefix(repo: &EntityRef) -> String {
    format!("{}%", escape_like(&repo.to_string()))
}

/// The escaping half of [`like_prefix`], without the trailing `%`.
///
/// Split out for #289, whose look-back for an action item's ticket narrowed to
/// one source's corpus with `<escaped source id>:%`. That read went in #1958:
/// an action item's ticket is now found by `sources::write_queue::ticket_filed`,
/// which matches `source_id` exactly and needs no pattern. The split stays
/// because the escaping is the part worth testing on its own.
fn escape_like(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 4);
    for ch in value.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

/// One entity id, or `None`.
///
/// Every value is a bound parameter and `sql` is a `&'static str` -- the rule
/// `commands/entity/mod.rs` states for this crate, which sqlx now enforces at the
/// type level.
async fn found(
    pool: &PgPool,
    sql: &'static str,
    prefix: &str,
    kind: &str,
    value: &str,
) -> Result<Option<String>, IpcError> {
    sqlx::query_scalar::<_, String>(sql)
        .bind(prefix)
        .bind(kind)
        .bind(value)
        .fetch_optional(pool)
        .await
        .map_err(IpcError::internal)
}

/// One pull request on a head, as the mirror and its source's declaration
/// leave it.
///
/// `merged` is `Option<bool>` and `state` `Option<String>` because both reads
/// **miss** rather than guess: an undeclared flag, a flag that is not a JSON
/// boolean, an absent or non-string `state`. Which of those it was is not a
/// distinction any caller may act on -- ADR-0007's requirement 1 -- so both
/// arrive as "or nothing".
#[derive(Clone, Debug, PartialEq, Eq, sqlx::FromRow)]
pub struct OnHead {
    pub entity_id: String,
    pub merged: Option<bool>,
    pub state: Option<String>,
}

/// What the mirror holds from head branch `head` into base branch `base`
/// ([`PULL_REQUEST_BY_HEAD_AND_BASE`], #1952), resolved through the declarations
/// `declarations` carries.
///
/// Public, and taking a pool rather than a [`Queue`], because this is the half
/// of the look-before-write that a test can drive against a real mirror: the
/// [`Steps`] fake stands where the queue does and therefore cannot witness the
/// statement above. `Queue::pull_request` is this function plus the lookup of
/// the declarations.
///
/// # Errors
///
/// [`Internal`](crate::IpcErrorCode::Internal) if the read fails.
pub async fn pull_request_on_head(
    pool: &PgPool,
    declarations: &Declarations,
    repo: &EntityRef,
    head: &str,
    base: &str,
) -> Result<PullRequestOnHead, IpcError> {
    let rows: Vec<OnHead> = sqlx::query_as(PULL_REQUEST_BY_HEAD_AND_BASE)
        .bind(like_prefix(repo))
        .bind(KIND_PR)
        .bind(head)
        .bind(declarations.as_param())
        .bind(base)
        .fetch_all(pool)
        .await
        .map_err(IpcError::internal)?;
    Ok(pick_open(&rows))
}

/// Which of a head's pull requests the flow may act on.
///
/// **The rule, written once**, because it has two callers: this module, over
/// what [`PULL_REQUEST_BY_HEAD_AND_BASE`] answered, and the [`Steps`] fake in
/// `tests/it/start_work.rs`, over the mirror a test dictated. A fake that
/// classified for itself would be a second opinion about what "open" means,
/// and the seam tests would then pass against a rule the real read does not
/// have. That is the whole reason this and [`pull_request_on_head`] are public
/// -- `queue` is the flow's IO half, not its policy, and nothing here widens
/// what the *orchestrator* next door touches.
///
/// Open is a thing knobas has to have **read**, not a thing it failed to
/// disprove: the record's own state has to say [`STATE_OPEN`], and the merged
/// flag the source declares must not say otherwise. Either read missing leaves
/// [`PullRequestOnHead::Unknown`]; see that type for why that direction.
#[must_use]
pub fn pick_open(rows: &[OnHead]) -> PullRequestOnHead {
    if let Some(row) = rows.iter().find(|row| is_open(row)) {
        return PullRequestOnHead::Open(row.entity_id.clone());
    }
    if let Some(row) = rows.iter().find(|row| is_finished(row)) {
        return PullRequestOnHead::Closed {
            id: row.entity_id.clone(),
            merged: row.merged == Some(true),
        };
    }
    PullRequestOnHead::Unknown
}

/// knobas read this record as open: its state says so and nothing knobas read
/// calls it merged.
fn is_open(row: &OnHead) -> bool {
    row.state.as_deref() == Some(STATE_OPEN) && row.merged != Some(true)
}

/// knobas read this record as **not** open -- a state that is not
/// [`STATE_OPEN`], or a declaration that calls it merged. A record whose state
/// knobas could not read and whose flag says nothing is neither this nor
/// [`is_open`]: it is the miss.
///
/// # The one thing an unfamiliar spelling costs, said plainly
///
/// A state word knobas does not know -- a forge that says `OPENED` -- lands
/// here rather than on the miss, because what knobas read is a record that
/// says something and does not say [`STATE_OPEN`]. That is the safe half:
/// both this and the miss refuse to link and both let the create step
/// dispatch, so an unfamiliar spelling can cost a refusal and can never cost a
/// wrong link. What it can be wrong about is the **word** the refusal uses --
/// [`super::link_step`] says "closed" wherever the declaration does not say
/// merged, and a forge whose `OPENED` means open would be called closed.
///
/// Left as it is on purpose: the alternative is to carry the record's own
/// state word into [`PullRequestOnHead::Closed`] so the refusal can quote it,
/// which is a wider type for a case no source in this repo can reach --
/// [`STATE_OPEN`] says why -- and the classification, which is what gates the
/// permanent act, is right either way.
fn is_finished(row: &OnHead) -> bool {
    row.merged == Some(true) || (row.state.is_some() && !is_open(row))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The look-before-write must not find another repository's branch. `_`
    /// matches any character in a `like` pattern, and repository names carry
    /// underscores all the time.
    #[test]
    fn a_repository_id_is_matched_literally_not_as_a_pattern() {
        let repo = EntityRef::new("gitea", "acme/payout_service");
        assert_eq!(like_prefix(&repo), "gitea:acme/payout\\_service%");
        let plain = EntityRef::new("gitea", "acme/payouts");
        assert_eq!(like_prefix(&plain), "gitea:acme/payouts%");
        // The same escaping a source id gets (#289), without the `%` that
        // would make a trailing one indistinguishable from an escaped one.
        assert_eq!(escape_like("wiki_two"), "wiki\\_two");
        assert_eq!(escape_like("odd%"), "odd\\%");
    }

    /// A held queue row, the way `write_queue::get` answers it.
    fn held(held_snapshot: Option<serde_json::Value>, source_enabled: bool) -> QueuedWrite {
        QueuedWrite {
            id: 3,
            source_id: "jira".to_owned(),
            entity_id: "jira:PAY-231".to_owned(),
            op: "transition".to_owned(),
            payload: serde_json::json!({}),
            target_snapshot: serde_json::json!({}),
            state: WriteState::Held,
            wait_reason: None,
            detail: Some("somebody moved it to Done".to_owned()),
            queued_at: chrono::Utc::now(),
            attempted_at: None,
            attempts: 1,
            held_snapshot,
            settled_at: None,
            source_enabled,
        }
    }

    /// The held side's `unconfirmed` key is what tells a send never answered
    /// from a target that changed (#1125, #1966); the step reads the first as
    /// its own landing, naming the source to check, and never as a drift
    /// (#1973). A source turned off is its own landing too (#204), and a
    /// held row with neither is the drift it always was.
    #[test]
    fn each_reason_a_write_is_held_is_its_own_landing() {
        let unconfirmed = write_queue::unconfirmed(
            "transition",
            serde_json::json!({}),
            "Jira",
            "timed out after 30s",
        );
        assert_eq!(
            landing_from(Some(held(Some(unconfirmed.clone()), true))),
            Landing::Unconfirmed {
                source: "Jira".to_owned(),
                detail: Some("timed out after 30s".to_owned()),
            }
        );
        // Turned off after the send: it may still have arrived, which is the
        // fact that decides whether a second send is safe.
        assert!(matches!(
            landing_from(Some(held(Some(unconfirmed), false))),
            Landing::Unconfirmed { .. }
        ));
        assert_eq!(landing_from(Some(held(None, false))), Landing::SourceOff);
        assert_eq!(
            landing_from(Some(held(
                Some(serde_json::json!({"op": "transition"})),
                true
            ))),
            Landing::Held {
                detail: Some("somebody moved it to Done".to_owned()),
            }
        );
        assert_eq!(landing_from(None), Landing::Withdrawn);
    }

    /// One mirrored pull request, spelled the way the statement answers it.
    fn row(id: &str, merged: Option<bool>, state: Option<&str>) -> OnHead {
        OnHead {
            entity_id: id.to_owned(),
            merged,
            state: state.map(str::to_owned),
        }
    }

    /// The pinned failure direction (ADR-0007 requirement 3, and #359's whole
    /// question): open is a thing knobas has to have **read**. A record whose
    /// state it could not read is `Unknown` however loudly the rest of the
    /// payload hints, so the flow dispatches and lets the source refuse a
    /// duplicate rather than settling a step against a record it cannot read.
    #[test]
    fn a_pull_request_of_unreadable_state_is_never_the_open_one() {
        assert_eq!(
            pick_open(&[row("gitea:acme/api#7", Some(false), None)]),
            PullRequestOnHead::Unknown,
            "a declaration saying `not merged` is not a record saying `open`"
        );
        assert_eq!(pick_open(&[]), PullRequestOnHead::Unknown);
    }

    /// **The bug the ticket names.** Lexicographic order over a re-used head
    /// puts the *merged* `#7` ahead of the open `#8`, so picking the first row
    /// is picking a merge. The open one is chosen however the rows are
    /// ordered.
    #[test]
    fn an_open_pull_request_wins_over_a_merged_one_from_the_same_head() {
        let merged_first = [
            row("gitea:acme/api#7", Some(true), Some("closed")),
            row("gitea:acme/api#8", Some(false), Some("open")),
        ];
        assert_eq!(
            pick_open(&merged_first),
            PullRequestOnHead::Open("gitea:acme/api#8".to_owned())
        );
        let merged_last = [
            row("gitea:acme/api#8", Some(false), Some("open")),
            row("gitea:acme/api#7", Some(true), Some("closed")),
        ];
        assert_eq!(
            pick_open(&merged_last),
            PullRequestOnHead::Open("gitea:acme/api#8".to_owned())
        );
    }

    /// **The bug one state over**, and the reason `state` decides rather than
    /// the declared flag: a pull request closed *without* merging carries
    /// `merged: false`, which a rule built on the declaration alone reads as
    /// open. It is not open, and knobas must not settle a step or draw a link
    /// against it.
    #[test]
    fn a_pull_request_closed_without_merging_is_not_open_either() {
        let closed = [row("gitea:acme/api#7", Some(false), Some("closed"))];
        assert_eq!(
            pick_open(&closed),
            PullRequestOnHead::Closed {
                id: "gitea:acme/api#7".to_owned(),
                merged: false,
            },
            "`merged: false` is not `state: open` -- somebody closed this one"
        );
    }

    /// A head whose every pull request is finished names one, and says which
    /// word the refusal should use, rather than answering the same "nothing
    /// here" an empty mirror does.
    #[test]
    fn a_head_whose_pull_requests_are_all_finished_names_one_and_says_which() {
        let merged = [
            row("gitea:acme/api#7", Some(true), Some("closed")),
            row("gitea:acme/api#8", Some(true), Some("closed")),
        ];
        assert_eq!(
            pick_open(&merged),
            PullRequestOnHead::Closed {
                id: "gitea:acme/api#7".to_owned(),
                merged: true,
            }
        );
    }

    /// **The declaration vetoes.** A source that says a record is merged
    /// settles it, whatever its own `state` string happens to say -- the
    /// declared flag is the one reading the source owns rather than knobas.
    #[test]
    fn a_declaration_that_says_merged_beats_a_state_that_says_open() {
        let contradictory = [row("gitea:acme/api#7", Some(true), Some("open"))];
        assert_eq!(
            pick_open(&contradictory),
            PullRequestOnHead::Closed {
                id: "gitea:acme/api#7".to_owned(),
                merged: true,
            }
        );
    }

    /// A source that declares no merged flag still resolves through `state`,
    /// which is the concrete reason the deciding fact is the record's own: a
    /// source may decline that declaration -- `knobas-source-mock` does, its
    /// merge being a timestamp -- and its pull requests still have to be
    /// linkable. The ids say `mock` for that shape and not because the mock
    /// reaches this read; [`STATE_OPEN`] records that it does not.
    #[test]
    fn a_source_that_declares_no_merged_flag_still_reads_its_state() {
        assert_eq!(
            pick_open(&[row("mock:acme/api#8", None, Some("open"))]),
            PullRequestOnHead::Open("mock:acme/api#8".to_owned())
        );
        assert_eq!(
            pick_open(&[row("mock:acme/api#7", None, Some("merged"))]),
            PullRequestOnHead::Closed {
                id: "mock:acme/api#7".to_owned(),
                merged: false,
            },
            "the word is `closed`: nothing knobas can read says this was merged"
        );
    }
}
