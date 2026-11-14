//! A pull request's view (#662, spec #660 stories 6-9 and 13): *Overview*,
//! *Files* (deferred), *Commits* and *Conversation*.
//!
//! The overview reads the pull request and its reviews; *Commits* its commit
//! listing; *Conversation* the discussion Gitea keeps on the issue of the same
//! index (the read the sync makes), with the reviews among it in time order
//! (#705). *Files* is the one expensive read -- the whole `.diff` -- so it is
//! a stub with a count until its tab is opened (ADR-0020 rule 3).

use knobas_source::SourceError;
use knobas_source::view::{Block, Fact, SourceView, Tone, ViewRow, ViewSection};

use super::rows::{self, ListedCommit};
use super::{fact, login, name_of, person_fact, when_of};
use crate::model::UserRef;
use crate::source::GiteaSource;

pub(crate) mod diffs;
mod label;
mod land;
mod maintain;
mod range;
mod request;
mod review;
mod threads;

const OVERVIEW: &str = "overview";
const FILES: &str = "files";
const COMMITS: &str = "commits";
const CONVERSATION: &str = "conversation";

/// The view of pull request `number` in `owner/repo`, or of its one section.
pub(super) async fn draw(
    source: &GiteaSource,
    entity: &str,
    owner: &str,
    repo: &str,
    number: u64,
    section: Option<&str>,
) -> Result<SourceView, SourceError> {
    let at = PullView {
        source,
        owner,
        repo,
        number,
    };
    let mut actions = Vec::new();
    let sections = match section {
        None => {
            // Each tab is its own reads over the one record, so they are read
            // side by side (#703); the first to fail fails the view. Who the
            // token is goes beside them: the review actions are not offered
            // on its own pull requests (#705). So do landing's reads (#708)
            // and the head commit's statuses (#710), which *Checks* draws and
            // landing holds against the base's required checks (#721).
            // And the repository's labels and its organisation's, which *Add
            // label…* offers (#1194, #1235).
            // And who a review can be requested from (#2303), for
            // *Request…*.
            //
            // In two rounds, not three (the priority lane, 2026-11-13): the
            // reads that need nothing of the record go beside it, and those
            // that need the record or the reviews, the reviews' line
            // comments among them, beside each other. A round is a forge's
            // answer, and a forge a sync is loading takes hundreds of
            // milliseconds over each.
            let (pull, reviews, comments, viewer, labels) = tokio::try_join!(
                at.pull(),
                at.reviews(),
                at.comments(),
                at.viewer(),
                at.labels(),
            )?;
            let (commits, landing, statuses, candidates, said) = tokio::try_join!(
                at.commits(Some(&pull)),
                at.landing(&pull),
                at.statuses(&pull),
                at.candidates(&pull),
                async { Ok(at.line_comments(&reviews).await) },
            )?;
            let landing = landing.with_statuses(statuses.as_ref());
            let checks = statuses.as_ref().and_then(super::checks::block_of);
            // The order the #701 ruling sets: what a reviewer says first,
            // then the maintenance (#706, *Update branch…* #708), *Merge…*
            // last.
            actions.extend(review::actions(&pull, viewer.as_deref()));
            actions.extend(maintain::actions(&pull));
            actions.push(label::action(&pull, at.owner, &labels));
            actions.extend(land::actions(&pull, &reviews, &landing));
            // Drawn in the Reviewers panel, not the toolbar (#2303).
            actions.extend(request::action(
                &pull,
                viewer.as_deref(),
                &candidates,
                &pull.requested(),
            ));
            vec![
                overview(&pull, &reviews, &landing, checks),
                files_stub(&pull),
                commits,
                conversation(&reviews, &comments, &said),
            ]
        }
        Some(OVERVIEW) => {
            let (pull, reviews) = tokio::try_join!(at.pull(), at.reviews())?;
            let (landing, statuses) = tokio::try_join!(at.landing(&pull), at.statuses(&pull))?;
            let landing = landing.with_statuses(statuses.as_ref());
            let checks = statuses.as_ref().and_then(super::checks::block_of);
            vec![overview(&pull, &reviews, &landing, checks)]
        }
        Some(FILES) => vec![at.files().await?],
        Some(COMMITS) => vec![at.commits(None).await?],
        Some(CONVERSATION) => {
            let (reviews, comments) = tokio::try_join!(at.reviews(), at.comments())?;
            let said = at.line_comments(&reviews).await;
            vec![conversation(&reviews, &comments, &said)]
        }
        Some(other) => {
            return Err(SourceError::protocol(format!(
                "gitea: a pull request's view has no section {other:?} (asked for {entity:?})"
            )));
        }
    };
    Ok(SourceView { sections, actions })
}

/// A pull request's whole view from its record as the mirror holds it
/// (ADR-0042), reading nothing: *Overview*'s state, author, branches, times
/// and description, and *Files*' count. What only Gitea's other reads hold --
/// reviews, checks, whether it can be merged, its commits and conversation --
/// is left out, the two tabs saying so in a muted line, and no action is
/// offered. `None` for a record that is not a pull request.
pub(crate) fn from_record(payload: &serde_json::Value) -> Option<SourceView> {
    // Every field of [`Pull`] is optional: a record is one that names its
    // number and its head.
    payload.get("number")?.as_u64()?;
    payload.get("head")?.as_object()?;
    let pull: Pull = serde_json::from_value(payload.clone()).ok()?;
    let mut blocks = vec![Block::Facts {
        facts: overview_facts(&pull, Vec::new()),
    }];
    let body = pull.body.as_deref().map(str::trim_end).unwrap_or_default();
    if !body.trim().is_empty() {
        blocks.push(Block::Text {
            text: body.to_owned(),
            markdown: true,
        });
    }
    let waiting = |id: &str, title: &str, what: &str| ViewSection {
        id: id.to_owned(),
        title: title.to_owned(),
        count: None,
        deferred: false,
        blocks: vec![super::rows::muted_line(format!(
            "The {what} are read from Gitea; the last sync holds none."
        ))],
    };
    Some(SourceView {
        sections: vec![
            ViewSection {
                id: OVERVIEW.to_owned(),
                title: "Overview".to_owned(),
                count: None,
                deferred: false,
                blocks,
            },
            files_stub(&pull),
            waiting(COMMITS, "Commits", "commits"),
            waiting(CONVERSATION, "Conversation", "reviews and comments"),
        ],
        actions: Vec::new(),
    })
}

/// The pull request one view is about.
struct PullView<'a> {
    source: &'a GiteaSource,
    owner: &'a str,
    repo: &'a str,
    number: u64,
}

impl PullView<'_> {
    async fn pull(&self) -> Result<Pull, SourceError> {
        let raw = self
            .source
            .client
            .pull(self.owner, self.repo, self.number)
            .await?;
        serde_json::from_value(raw).map_err(|error| {
            SourceError::protocol(format!(
                "gitea: {}/{}#{} is not a pull request record: {error}",
                self.owner, self.repo, self.number
            ))
        })
    }

    /// Every review Gitea lists, oldest first; one it cannot read as a review
    /// is left out.
    async fn reviews(&self) -> Result<Vec<Review>, SourceError> {
        let reviews = self
            .source
            .client
            .pull_reviews(self.owner, self.repo, self.number)
            .await?;
        Ok(reviews
            .into_iter()
            .filter_map(|r| serde_json::from_value(r).ok())
            .collect())
    }

    /// Each review's line comments, by review id, for *Conversation* (#2277):
    /// a reply to a thread is a review of one line comment, and it is said
    /// there as Gitea's own page says it. Only the reviews Gitea counts any
    /// for are asked, all at once. A failed read quotes nothing: it costs the
    /// quotes, never the pull request's page (the Files tab still draws the
    /// threads, or says why it cannot).
    async fn line_comments(&self, reviews: &[Review]) -> Vec<(u64, Vec<String>)> {
        let commented = threads::with_comments(reviews);
        let reads = commented
            .iter()
            .map(|&(id, _)| {
                self.source
                    .client
                    .pull_review_comments(self.owner, self.repo, self.number, id)
            })
            .collect();
        let Ok(answers) = threads::all(reads).await else {
            return Vec::new();
        };
        commented
            .iter()
            .zip(answers)
            .map(|(&(id, review), raw)| {
                (
                    id,
                    threads::written(review, raw)
                        .iter()
                        .map(threads::quoted)
                        .collect(),
                )
            })
            .collect()
    }

    /// The diff, its reviews' line comments on their lines, and whether the
    /// reader may add more (#709): the pull request's state and its reviews
    /// are read beside the diff, then each review that has line comments, all
    /// at once.
    ///
    /// The commits the diff is between go with it (#2273, #2310): its merge
    /// base and head from the record, its commits, and the head the viewer
    /// last reviewed at, read beside the rest. The commits are for picking
    /// from, not the diff: a failed read of them lists none and fails nothing.
    ///
    /// The `.diff` is read after the commits, and not at all where one is
    /// kept at the head they end on (`diffs`, #2321).
    async fn files(&self) -> Result<ViewSection, SourceError> {
        let (pull, (commits, fetched), reviews, resolves, viewer) = tokio::try_join!(
            self.pull(),
            async {
                let commits = self.listed().await.map(|(c, _, _)| c).unwrap_or_default();
                let kept = commits.first().is_some_and(|tip| {
                    self.source.pull_diffs.holds_at(
                        self.owner,
                        self.repo,
                        self.number,
                        &tip.record.sha,
                    )
                });
                let fetched = if kept {
                    None
                } else {
                    Some(
                        self.source
                            .client
                            .pull_diff(self.owner, self.repo, self.number)
                            .await?,
                    )
                };
                Ok((commits, fetched))
            },
            self.reviews(),
            async { Ok(self.source.resolves().await) },
            self.viewer(),
        )?;
        let (text, unpinned) = self.diff(&pull, &commits, fetched).await?;
        let reviewed = range::reviewed(&reviews, viewer.as_deref());
        let range = range::of(
            &self.source.id,
            self.owner,
            self.repo,
            &pull,
            &commits,
            reviewed,
        );
        let (mut files, truncated) = knobas_source::diff::parse_view(&text);
        let commented = threads::with_comments(&reviews);
        let reads = commented
            .iter()
            .map(|&(id, _)| {
                self.source
                    .client
                    .pull_review_comments(self.owner, self.repo, self.number, id)
            })
            .collect();
        // The record read that pins a fetched diff runs beside the line
        // comments: it only has to start after the `.diff` answered.
        let (answers, ()) = tokio::join!(threads::all(reads), async {
            if let Some(key) = unpinned {
                self.keep(&pull, key, std::sync::Arc::clone(&text)).await;
            }
        });
        let answers = answers?;
        let written = commented
            .iter()
            .zip(answers)
            .flat_map(|(&(_, review), raw)| threads::written(review, raw))
            .collect();
        let gone = threads::place(&mut files, written);
        Ok(ViewSection {
            id: FILES.to_owned(),
            title: "Files".to_owned(),
            count: Some(files.len() as u64),
            // The read the stub deferred: these are its blocks, now.
            deferred: false,
            blocks: vec![Block::Diff {
                files,
                truncated,
                commentable: review::commenting(&pull, resolves),
                gone,
                range,
            }],
        })
    }

    /// The diff *Files* draws: `fetched`, read after `commits` was, or the
    /// one kept at the merge base the record names and the head `commits`
    /// ends on. A diff read here comes with the key it is kept under once
    /// [`Self::keep`] has pinned it; a kept one, or one with nothing to say
    /// what it stands at, comes with none.
    async fn diff(
        &self,
        pull: &Pull,
        commits: &[rows::ListedCommit],
        fetched: Option<String>,
    ) -> Result<(std::sync::Arc<str>, Option<diffs::DiffKey>), SourceError> {
        let head = pull.head.as_ref().and_then(|h| h.sha.as_deref());
        let tip = commits.first().map(|c| c.record.sha.as_str());
        let key = match (pull.merge_base.as_deref(), tip) {
            (Some(base), Some(tip)) if head.is_some_and(|h| h.eq_ignore_ascii_case(tip)) => {
                diffs::DiffKey::of(self.owner, self.repo, self.number, base, tip)
            }
            _ => None,
        };
        let fetched = match fetched {
            Some(text) => text,
            None => match key.as_ref().and_then(|k| self.source.pull_diffs.get(k)) {
                Some(text) => return Ok((text, None)),
                None => {
                    self.source
                        .client
                        .pull_diff(self.owner, self.repo, self.number)
                        .await?
                }
            },
        };
        Ok((fetched.into(), key))
    }

    /// Keep `text`, a `.diff` read after the listing named `key`'s head,
    /// where it is sure to stand at that head: the `.diff` is at that head or
    /// a later one; the record names the branch's tip, which the `.diff`
    /// never runs ahead of, so a record read after the `.diff` that still
    /// names that head and merge base pins it. A failed read keeps nothing.
    async fn keep(&self, pull: &Pull, key: diffs::DiffKey, text: std::sync::Arc<str>) {
        let head = pull.head.as_ref().and_then(|h| h.sha.as_deref());
        let after = self.pull().await.ok();
        let same = after.is_some_and(|after| {
            after.merge_base == pull.merge_base
                && after.head.as_ref().and_then(|h| h.sha.as_deref()) == head
        });
        if same {
            self.source.pull_diffs.put(key, text);
        }
    }

    /// The pull request's commits, at most [`rows::COMMIT_CAP`], ending on
    /// a note where Gitea has more (#678).
    async fn commits(&self, pull: Option<&Pull>) -> Result<ViewSection, SourceError> {
        let (records, total, cut) = self.listed().await?;
        let mut rows: Vec<ViewRow> = records
            .iter()
            .map(|c| rows::commit_row(&self.source.id, self.owner, self.repo, &c.record))
            .collect();
        // The count and the note both say what the tab lists; a record that
        // did not parse is one the list does not hold.
        let listed = rows.len();
        if let Some(mut note) = rows::left_out("commits", listed, total, cut, None) {
            // Where the rest is: the pull request's own page, read for the
            // note when this tab was asked for on its own.
            // A link is not worth failing the tab over: without it the note
            // still says the list was cut.
            let page = match pull {
                Some(pull) => pull.html_url.clone(),
                None => self.pull().await.ok().and_then(|p| p.html_url),
            };
            note.url = page.and_then(|page| rows::under(&page, "/commits"));
            rows.push(note);
        }
        Ok(ViewSection {
            id: COMMITS.to_owned(),
            title: "Commits".to_owned(),
            count: Some(listed as u64),
            deferred: false,
            blocks: vec![Block::Entities { title: None, rows }],
        })
    }

    /// The pull request's commits as Gitea lists them, newest first, at most
    /// [`rows::COMMIT_CAP`]; a record that does not parse is left out. With
    /// them the server's own count, where it sent one, counting the records
    /// read where they outnumber it, and whether the cap cut the walk.
    async fn listed(&self) -> Result<(Vec<ListedCommit>, Option<usize>, bool), SourceError> {
        let mut records = Vec::new();
        let mut total = None;
        let mut cut = true;
        for page in 1..=rows::COMMIT_PAGES {
            let (batch, count) = self
                .source
                .client
                .pull_commits(self.owner, self.repo, self.number, page)
                .await?;
            total = count.or(total);
            // Ends on an empty page or on the server's own count, never on a
            // short page (`client::PAGE_SIZE`'s rule).
            let done = batch.is_empty();
            records.extend(batch);
            if done || total.is_some_and(|t| records.len() >= t) {
                cut = false;
                break;
            }
        }
        let read = records.len();
        let total = total.map(|t| t.max(read));
        let records = records
            .into_iter()
            .filter_map(|r| serde_json::from_value::<ListedCommit>(r).ok())
            .collect();
        Ok((records, total, cut))
    }

    /// The discussion Gitea keeps on the issue of the same index, oldest
    /// first; a comment it cannot read is left out. A token Gitea refuses the
    /// issue's read for its scope reads the rest of the view, so that refusal
    /// is kept as the reason *Conversation* gives (#2042); any other failure
    /// is the view's.
    async fn comments(&self) -> Result<Result<Vec<PullComment>, String>, SourceError> {
        let discussion = match self
            .source
            .client
            .issue_comments(self.owner, self.repo, self.number)
            .await
        {
            Ok(discussion) => discussion,
            Err(error) => {
                let what = "this pull request's comments";
                return match super::grants::unread(&error, what, super::grants::READ_ISSUE) {
                    Some(reason) => Ok(Err(reason)),
                    None => Err(error),
                };
            }
        };
        Ok(Ok(discussion
            .raw
            .into_iter()
            .filter_map(|c| serde_json::from_value(c).ok())
            .collect()))
    }

    /// The head commit's own statuses (#710), by the sha the record names;
    /// nothing where it names none. Never an error: a failed read is kept,
    /// drawn as a line by *Checks* and deciding nothing for landing, so this
    /// sits in the `try_join!` as a success.
    async fn statuses(
        &self,
        pull: &Pull,
    ) -> Result<Option<Result<Vec<super::checks::Status>, SourceError>>, SourceError> {
        Ok(match pull.head.as_ref().and_then(|h| h.sha.as_deref()) {
            Some(sha) => Some(super::checks::read(self.source, self.owner, self.repo, sha).await),
            None => None,
        })
    }

    /// The repository's labels and its organisation's, for *Add label…*
    /// (#1194, #1235). A failed read costs that action, which says so, and
    /// never the view.
    async fn labels(&self) -> Result<Result<Vec<label::Label>, String>, SourceError> {
        Ok(label::read(self.source, self.owner, self.repo).await)
    }

    /// Who a review of `pull` can be requested from (#2303): asked only on
    /// an open pull request, and a failed read is nobody, which costs
    /// *Request…* and never the view.
    async fn candidates(&self, pull: &Pull) -> Result<Vec<UserRef>, SourceError> {
        if !pull.is_open() {
            return Ok(Vec::new());
        }
        Ok(self
            .source
            .client
            .repo_reviewers(self.owner, self.repo)
            .await
            .unwrap_or_default())
    }

    /// The login the token belongs to, or `None` where Gitea would not say
    /// ([`GiteaSource::viewer`], read once per token and server).
    async fn viewer(&self) -> Result<Option<String>, SourceError> {
        Ok(self.source.viewer().await)
    }
}

/// The discussion: each comment and each review, in time order, as its
/// author and time -- a review's verdict beside them -- and then its text
/// (issue #705).
///
/// Gitea keeps the comments on the issue of the same index and the reviews on
/// the pull request; the two lists are merged by the instant each was made,
/// and a tie keeps the order Gitea gave (reviews first). A draft (`PENDING`)
/// is nobody's yet and a review request (`REQUEST_REVIEW`) is not something
/// anyone said, so neither is drawn.
///
/// Comments the token may not read (`Err`, its reason) open the tab on a line
/// saying so, and the reviews, which are the pull request's own read, follow
/// (#2042).
fn conversation(
    reviews: &[Review],
    comments: &Result<Vec<PullComment>, String>,
    line_comments: &[(u64, Vec<String>)],
) -> ViewSection {
    let (unread, comments) = match comments {
        Ok(comments) => (None, comments.as_slice()),
        Err(reason) => (Some(reason), &[][..]),
    };
    let mut said: Vec<Said<'_>> = reviews
        .iter()
        .filter_map(|review| {
            let verdict = review_verdict(review)?;
            let lines = review
                .id
                .and_then(|id| line_comments.iter().find(|(r, _)| *r == id))
                .map(|(_, lines)| lines.as_slice())
                .unwrap_or_default();
            Some(Said {
                user: review.user.as_ref(),
                at: review.submitted_at.as_deref(),
                verdict: Some(verdict),
                body: review.body.as_deref(),
                lines,
            })
        })
        .chain(comments.iter().map(|comment| Said {
            user: comment.user.as_ref(),
            at: comment.created_at.as_deref(),
            verdict: None,
            body: comment.body.as_deref(),
            lines: &[],
        }))
        .collect();
    // Stable: equal instants, and entries with none (last), keep Gitea's
    // order.
    said.sort_by_key(|entry| {
        let instant = entry
            .at
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok());
        (instant.is_none(), instant)
    });
    let mut blocks = Vec::with_capacity(said.len() * 2 + 1);
    blocks.extend(unread.map(|reason| super::rows::muted_line(reason.clone())));
    for entry in &said {
        // Named as a ticket's comments name their authors (F-107), the
        // login beside the name for the reader's own.
        let who = name_of(entry.user).unwrap_or_else(|| "someone".to_owned());
        let when = entry.at.map(when_of).unwrap_or_default();
        let mut facts = vec![person_fact(&who, when, entry.user)];
        if let Some((value, tone)) = &entry.verdict {
            facts.push(Fact {
                label: "Review".to_owned(),
                value: value.clone(),
                tone: *tone,
                account: None,
            });
        }
        blocks.push(Block::Facts { facts });
        // The review's words, then its line comments, each quoted under
        // where it was written (#2277): one text, as the message's body.
        let body = entry.body.map(str::trim_end).unwrap_or_default();
        let text: Vec<&str> = std::iter::once(body)
            .filter(|b| !b.trim().is_empty())
            .chain(entry.lines.iter().map(String::as_str))
            .collect();
        if !text.is_empty() {
            blocks.push(Block::Text {
                text: text.join("\n\n"),
                markdown: true,
            });
        }
    }
    ViewSection {
        id: CONVERSATION.to_owned(),
        title: "Conversation".to_owned(),
        count: Some(said.len() as u64),
        deferred: false,
        blocks,
    }
}

/// One thing said in *Conversation*: a comment, or a review with its verdict.
struct Said<'a> {
    user: Option<&'a UserRef>,
    at: Option<&'a str>,
    verdict: Option<(String, Option<Tone>)>,
    body: Option<&'a str>,
    /// A review's line comments, quoted ([`threads::quoted`]).
    lines: &'a [String],
}

/// What a review says in *Conversation*, and its tone: an approval ok, a
/// request for changes warn, a plain comment none, and a dismissed review
/// muted whatever it said. `None` for what is not part of the conversation:
/// a draft, a review request, a state Gitea did not send.
fn review_verdict(review: &Review) -> Option<(String, Option<Tone>)> {
    let (word, tone) = match review.state.as_deref()? {
        "APPROVED" => ("approved", Some(Tone::Ok)),
        "REQUEST_CHANGES" => ("changes requested", Some(Tone::Warn)),
        "COMMENT" => ("commented", None),
        "PENDING" | "REQUEST_REVIEW" | "" => return None,
        other => return Some((other.to_lowercase().replace('_', " "), None)),
    };
    if review.dismissed {
        return Some((format!("{word}, dismissed"), Some(Tone::Muted)));
    }
    Some((word.to_owned(), tone))
}

/// The facts, each reviewer's standing verdict, the head commit's checks
/// (#710) and the description.
fn overview(
    pull: &Pull,
    reviews: &[Review],
    landing: &land::Landing,
    checks: Option<Block>,
) -> ViewSection {
    let landing_facts = land::mergeable_fact(pull, reviews, landing)
        .into_iter()
        .chain(land::ready_facts(pull, reviews, landing))
        .collect();
    let mut facts = overview_facts(pull, landing_facts);
    facts.extend(verdicts(reviews));
    let mut blocks = vec![Block::Facts { facts }];
    blocks.extend(checks);
    let body = pull.body.as_deref().map(str::trim_end).unwrap_or_default();
    if !body.trim().is_empty() {
        blocks.push(Block::Text {
            text: body.to_owned(),
            markdown: true,
        });
    }
    ViewSection {
        id: OVERVIEW.to_owned(),
        title: "Overview".to_owned(),
        count: None,
        deferred: false,
        blocks,
    }
}

/// The *Files* tab before it is opened: its count, from the pull request's
/// own `changed_files`, and no blocks.
fn files_stub(pull: &Pull) -> ViewSection {
    ViewSection {
        id: FILES.to_owned(),
        title: "Files".to_owned(),
        count: pull.changed_files,
        deferred: true,
        blocks: Vec::new(),
    }
}

/// State, author, `head → base`, whether it can be merged (`landing`:
/// *Mergeable*, *Approvals* and *Conflicts*, [`land`]'s to say), its times.
fn overview_facts(pull: &Pull, landing: Vec<Fact>) -> Vec<Fact> {
    let (state, tone) = if pull.merged {
        ("merged".to_owned(), Some(Tone::Ok))
    } else {
        let word = pull.state.clone().unwrap_or_default();
        let tone = match word.as_str() {
            "open" => Some(Tone::Info),
            "closed" => Some(Tone::Muted),
            _ => None,
        };
        (word, tone)
    };
    let mut facts = Vec::new();
    if !state.is_empty() {
        facts.push(Fact {
            label: "State".to_owned(),
            value: state.clone(),
            tone,
            account: None,
        });
    }
    if let Some(author) = name_of(pull.user.as_ref()) {
        facts.push(person_fact("Author", author, pull.user.as_ref()));
    }
    if let (Some(head), Some(base)) = (pull.head.as_ref(), pull.base.as_ref())
        && let (Some(head_ref), Some(base_ref)) = (head.r#ref.as_deref(), base.r#ref.as_deref())
    {
        let head_repo = head.repo.as_ref().and_then(|r| r.full_name.as_deref());
        let base_repo = base.repo.as_ref().and_then(|r| r.full_name.as_deref());
        let head_name = match head_repo {
            Some(fork) if Some(fork) != base_repo => format!("{fork}:{head_ref}"),
            _ => head_ref.to_owned(),
        };
        facts.push(fact("Branches", format!("{head_name} → {base_ref}")));
    }
    facts.extend(landing);
    if let Some(at) = pull.created_at.as_deref() {
        facts.push(fact("Created", when_of(at)));
    }
    if let Some(at) = pull.updated_at.as_deref() {
        facts.push(fact("Updated", when_of(at)));
    }
    if pull.merged
        && let Some(at) = pull.merged_at.as_deref()
    {
        let value = match name_of(pull.merged_by.as_ref()) {
            Some(by) => format!("{} by {by}", when_of(at)),
            None => when_of(at),
        };
        facts.push(fact("Merged", value));
    }
    facts
}

/// Each reviewer's standing verdict, in the order reviewers first appear.
///
/// A reviewer's latest review is their verdict, with three exceptions: a
/// draft (`PENDING`) is not yet anyone's verdict, a dismissed review leaves
/// its reviewer none (an earlier verdict does not come back: Gitea counts only
/// a reviewer's latest review), and a plain comment after a verdict does not
/// take it back.
fn verdicts(reviews: &[Review]) -> Vec<Fact> {
    // By login, as Gitea counts a reviewer, each with the review's account.
    let mut standing: Vec<(String, String, Option<&UserRef>)> = Vec::new();
    for review in reviews {
        let Some(who) = login(review.user.as_ref()) else {
            continue;
        };
        let state = review.state.as_deref().unwrap_or_default();
        if review.dismissed {
            standing.retain(|(name, _, _)| *name != who);
            continue;
        }
        if state.is_empty() || state == "PENDING" {
            continue;
        }
        match standing.iter_mut().find(|(name, _, _)| *name == who) {
            Some(_) if state == "COMMENT" => {}
            Some((_, verdict, _)) => state.clone_into(verdict),
            None => standing.push((who, state.to_owned(), review.user.as_ref())),
        }
    }
    standing
        .into_iter()
        .map(|(login, state, user)| {
            // Named as a ticket names its people (F-107); the login stays
            // the fact's account.
            let who = name_of(user).unwrap_or(login);
            let (value, tone) = match state.as_str() {
                "APPROVED" => (format!("approved by {who}"), Some(Tone::Ok)),
                "REQUEST_CHANGES" => (format!("changes requested by {who}"), Some(Tone::Warn)),
                "REQUEST_REVIEW" => (format!("requested from {who}"), Some(Tone::Muted)),
                "COMMENT" => (format!("commented by {who}"), None),
                other => (
                    format!("{} by {who}", other.to_lowercase().replace('_', " ")),
                    None,
                ),
            };
            Fact {
                tone,
                ..person_fact("Review", value, user)
            }
        })
        .collect()
}

impl Pull {
    /// Open and not merged: what reviews, the maintenance's *Close* and
    /// landing all ask first.
    fn is_open(&self) -> bool {
        !self.merged && self.state.as_deref() == Some("open")
    }

    /// The head branch's name.
    fn head_ref(&self) -> Option<&str> {
        self.head.as_ref().and_then(|b| b.r#ref.as_deref())
    }

    /// The base branch's name.
    fn base_ref(&self) -> Option<&str> {
        self.base.as_ref().and_then(|b| b.r#ref.as_deref())
    }

    /// Whether the token's account may push to the base repository, as
    /// Gitea says on the record (`base.repo.permissions.push`).
    fn may_push(&self) -> bool {
        self.base
            .as_ref()
            .and_then(|b| b.repo.as_ref())
            .and_then(|r| r.permissions.as_ref())
            .is_some_and(|p| p.push)
    }

    /// The logins a review is requested from now (`requested_reviewers`).
    fn requested(&self) -> Vec<String> {
        self.requested_reviewers
            .iter()
            .flatten()
            .filter_map(|u| login(Some(u)))
            .collect()
    }

    /// The head lives in the base's repository, not a fork's.
    fn head_in_base_repo(&self) -> bool {
        let repo = |b: &Option<PullBranch>| {
            b.as_ref()
                .and_then(|b| b.repo.as_ref())
                .and_then(|r| r.full_name.clone())
        };
        repo(&self.head) == repo(&self.base)
    }
}

/// The fields of Gitea's `PullRequest` this view reads.
#[derive(Debug, serde::Deserialize)]
struct Pull {
    /// Its index, which a squash's default message ends on (#843).
    #[serde(default)]
    number: Option<u64>,
    /// What *Edit…* prefills (#706).
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    merged: bool,
    #[serde(default)]
    mergeable: Option<bool>,
    #[serde(default)]
    user: Option<UserRef>,
    #[serde(default)]
    merged_by: Option<UserRef>,
    #[serde(default)]
    head: Option<PullBranch>,
    #[serde(default)]
    base: Option<PullBranch>,
    /// Gitea types the times as `date-time`; kept as text for [`when_of`],
    /// which shows the offset the server sent.
    #[serde(default)]
    created_at: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    merged_at: Option<String>,
    #[serde(default)]
    changed_files: Option<u64>,
    #[serde(default)]
    html_url: Option<String>,
    /// The labels it carries, which *Add label…* does not offer again
    /// (#1194).
    #[serde(default)]
    labels: Vec<label::Label>,
    /// The commit the head forked from the base at, which the diff is from
    /// (#2310).
    #[serde(default)]
    merge_base: Option<String>,
    /// Who a review is requested from now (#2303); `null` reads as none.
    #[serde(default)]
    requested_reviewers: Option<Vec<UserRef>>,
}

#[derive(Debug, serde::Deserialize)]
struct PullBranch {
    #[serde(default)]
    r#ref: Option<String>,
    /// The head's tip, which landing asks the base about (#708), and whose
    /// statuses are the checks (#710).
    #[serde(default)]
    sha: Option<String>,
    #[serde(default)]
    repo: Option<PullRepo>,
}

#[derive(Debug, serde::Deserialize)]
struct PullRepo {
    #[serde(default)]
    full_name: Option<String>,
    /// What the token's account may do there (#2303).
    #[serde(default)]
    permissions: Option<RepoPermissions>,
}

/// Gitea's `Permission` on a repository record.
#[derive(Debug, serde::Deserialize)]
struct RepoPermissions {
    #[serde(default)]
    push: bool,
}

#[derive(Debug, serde::Deserialize)]
struct Review {
    /// What its line comments are read by (#709).
    #[serde(default)]
    id: Option<u64>,
    /// How many line comments Gitea counts on it: none is no read.
    #[serde(default)]
    comments_count: u64,
    #[serde(default)]
    user: Option<UserRef>,
    /// The team a review was requested from, where it was a team's
    /// (#721: *Merge…* names who it waits for).
    #[serde(default)]
    team: Option<TeamRef>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    dismissed: bool,
    /// Counted against a protection's required approvals (#708).
    #[serde(default)]
    official: bool,
    /// Given before the head last moved; not counted where the protection
    /// ignores stale approvals (#708).
    #[serde(default)]
    stale: bool,
    #[serde(default)]
    body: Option<String>,
    /// As for [`Pull`]'s times, kept as text for [`when_of`].
    #[serde(default)]
    submitted_at: Option<String>,
    /// The head it was given at (#2273).
    #[serde(default)]
    commit_id: Option<String>,
}

/// Gitea's `Team`, the field a review request names it by.
#[derive(Debug, serde::Deserialize)]
struct TeamRef {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct PullComment {
    #[serde(default)]
    user: Option<UserRef>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    created_at: Option<String>,
}
