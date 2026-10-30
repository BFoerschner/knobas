// Generated from the Rust types by ts-rs; do not edit. Regenerate with `cargo test -p knobas-app --lib ipc_mirror` (crates/knobas-app/src/ipc_mirror.rs).
import type { BuildParameter, ReviewSide, Trigger, WriteState } from "./sources";

/**
 * One input of a [`ViewAction`]'s dialog. The reader's answer reaches the
 * shell's payload table as text under `name`.
 */
export type ActionField = { 
/**
 * The key the value is handed on under, in the op's own words
 * (`"style"`, `"delete_branch"`). Unique within one action.
 */
name: string, 
/**
 * The field's label, in the reader's language.
 */
label: string, 
/**
 * How it asks: one line, several, one of a few, or a checkbox.
 */
kind: FieldKind, 
/**
 * What the field starts with: a text's prefill, a choice's preselected
 * option (`None` picks the first), a flag's `"true"` (`None` or anything
 * else starts unchecked). A flag answers `"true"` or `"false"`.
 */
value: string | null, 
/**
 * The dialog will not send without it: a text with something other than
 * blanks in it, a choice with an option picked, a flag checked.
 */
required: boolean, 
/**
 * What an empty text field says (`"default branch"`).
 */
placeholder: string | null, };

/**
 * What became of filing a ticket from an action item.
 */
export type ActionItemTicket = { 
/**
 * The write queue row carrying the create.
 */
write_id: number, 
/**
 * The ticket's title as filed: the action item's words, trimmed. What
 * the tab matches an item by when it reads a protocol's
 * [`owed_tickets`](crate::protocol::Protocol::owed_tickets) (#1951).
 */
title: string, 
/**
 * The queue's own word for where that write stands (#779): `pending`
 * while it is on its way, `sent` once it landed, `refused` or `held`
 * when it will not go on its own, `discarded` once withdrawn.
 */
state: WriteState, 
/**
 * Whether the create is held as one its source may have taken (#1125):
 * sent once and never answered, so the ticket may exist. Such a create
 * is on its way until the reader sends it again or discards it, and the
 * tab offers no second *Create ticket* for it (Björn's ruling on #1966);
 * `false` for every other state, a drift hold included.
 */
may_have_landed: boolean, 
/**
 * The queue's or the source's sentence about the write. Untrusted text.
 */
detail: string | null, 
/**
 * The ticket, as an entity id, once the mirror has it.
 *
 * `None` for a create that is still queued, and for one that landed at a
 * source whose re-read has not brought the ticket back yet. Both are "not
 * yet", and the note is unlinked in both -- there is nothing to link to.
 */
ticket_entity_id: string | null, 
/**
 * Whether the note and the ticket are linked.
 */
linked: boolean, };

/**
 * One line of the activity log.
 */
export type ActivityRow = { id: number, 
/**
 * RFC 3339 on the wire.
 */
at: string, 
/**
 * `"user"`; `"knobas"` for an action knobas took on its own (the
 * start-work flow's reverse direction); or `"sync:<source_id>"` for a
 * synced event.
 */
actor: string, verb: string, entity_id: string | null, 
/**
 * Free-form jsonb, so §2.6 maps it to `unknown`.
 *
 * Not `Record<string, unknown>`: the Rust type is any JSON value, and only
 * `activity::insert` coerces a JSON `null` to `{}`. A row written with a
 * string, a number or an array is well-typed here and would make the
 * narrower declaration a lie, the kind that type-checks in the frontend
 * and throws at runtime. Narrow it with a check where you read it.
 */
detail: unknown, };

/**
 * One entry of a run's artifact folder (#856).
 */
export type ArtifactEntry = { 
/**
 * From the artifacts' root, `/`-separated, no leading `/`.
 */
path: string, name: string, 
/**
 * In bytes; `None` for a folder.
 */
size: number | null, folder: boolean, 
/**
 * How many entries a folder holds, where the source says.
 */
children: number | null, };

/**
 * The head of a text artifact (#856).
 */
export type ArtifactPreview = { 
/**
 * The text, at most [`PREVIEW_BYTES_MAX`] bytes of it; `None` for a file
 * that is not text.
 */
text: string | null, 
/**
 * The file is longer than [`text`](Self::text).
 */
truncated: boolean, };

/**
 * Which of the Code tile's groups a row is drawn under (#712): what needs
 * me first. Under [`EntityOrder::Attention`], in this order.
 *
 * "Me" is the row's **own source's** configured username, trimmed, and
 * compared case-sensitively as the inbox and the standup compare it. A pull request
 * and a branch are knobas' own words (`0001_init.sql`), and the inbox's
 * review rule names `pr` the same way.
 *
 * - `review_requested`: Open pull requests whose declared reviewers include me
 *   (Gitea's `requested_reviewers`, as the inbox reads it). Whether I reviewed
 *   since the last push is not in the mirror -- it holds no reviews and no
 *   push times -- so #712's clause asking it is dropped.
 * - `my_pull_requests`: Open pull requests I opened.
 * - `open_pull_requests`: Every other open pull request.
 * - `my_branches`: Branches whose last commit is mine and that no open pull
 *   request of the same repo is opened from (its declared `head`).
 * - `branches`: Branches of a source with no configured identity, in a room
 *   where another source has one (ruled 2026-10-13 on #712): nobody can say
 *   whose they are, so they are not hidden as somebody else's. The tile draws
 *   them as it drew every branch before, newest first.
 * - `other_branches`: Every other branch: somebody else's, or one an open pull
 *   request already stands for. The tile draws these only maximised.
 */
export type AttentionGroup = "review_requested" | "my_pull_requests" | "open_pull_requests" | "my_branches" | "branches" | "other_branches";

/**
 * One half of the uncommitted diff, as a diff block carries it.
 *
 * Issue #824.
 */
export type CappedDiff = { files: Array<DiffFile>, 
/**
 * A cap cut this half: the per-view cap of the parser, or the read cap
 * that stopped git.
 */
truncated: boolean, };

/**
 * What the main window last told this module about where the reader is.
 *
 * Both halves are optional and their absences mean different things, which is
 * why this is a pair of `Option`s rather than one `Option` of a pair: a
 * **derived** room -- *All work*, a source, a project -- has no context at
 * all, and a reader browsing with nothing open has no foreground, and either
 * can be true without the other.
 *
 * Issue #503. The two ids a capture attaches, and nothing else: `context` is
 * the **stored** room's `ctx:` entity; `foreground` is the **foreground** as
 * `CONTEXT.md`'s *Passive attribution* defines it, which is `shell/timer.ts`'s
 * `roomForeground` and no second spelling of it.
 */
export type CaptureContext = { 
/**
 * The `ctx:` entity of the last **stored** room, or `None`.
 */
context: string | null, 
/**
 * The foreground entity, or `None`.
 */
foreground: string | null, };

/**
 * The shortcut as the settings pane draws it.
 *
 * Two fields and three states, and the third is the one this shape exists for:
 * nothing stored (`accelerator: None`), stored and registered
 * (`refusal: None`), and stored but **not** registered, which is an
 * accelerator and a sentence saying why.
 *
 * Issue #503. With nothing stored no key is taken from any other application
 * until somebody chooses one. A refusal is the sentence to show -- the plugin
 * could not parse the accelerator, or the operating system would not give the
 * combination up -- because *which* refusal it was is the whole of what the
 * reader can act on.
 */
export type CaptureShortcut = { 
/**
 * What is stored, or `None` when nothing is -- the default, in which case
 * no shortcut is registered and knobas has taken no key from anybody.
 */
accelerator: string | null, 
/**
 * Why the stored accelerator is not registered, or `None` when it is (or
 * when there is nothing to register, or notes are hidden).
 */
refusal: string | null, 
/**
 * *Show notes and quick capture* (#832). Off, the stored accelerator is
 * kept and not registered.
 *
 * D10. Off, `refusal` is `None` too.
 */
shown: boolean, 
/**
 * How many notes there are (#832): Settings › Notes and capture draws
 * the count beside the switch, from this one read.
 */
note_count: number, 
/**
 * When the newest note was last changed, or `None` with no notes.
 *
 * RFC 3339 on the wire.
 */
last_note_at: string | null, };

/**
 * One pipeline of a chain (#857).
 */
export type ChainStep = { 
/**
 * The pipeline's [`EntityRef`](knobas_spi_types::entity::EntityRef) in string
 * form, of the same source.
 */
pipeline: string, 
/**
 * What the source calls the pipeline.
 */
name: string, };

/**
 * One step of the chain a run is in (#857, R-9.13): the step's pipeline,
 * its name in the source's word, and its run on the run's branch read
 * through the run kind's declarations (`None` where it has not run there).
 */
export type ChainStepRow = { pipeline_id: string, name: string, run: RunRow | null, };

/**
 * One application on the *Open in…* menu.
 *
 * Issue #844, R-8.21. Its commands take `{path}`, `{file}`, `{line}` and
 * `{name}`, all filled from this disk.
 */
export type CheckoutApplication = { 
/**
 * Stable id: what `open_checkout` takes and the default names.
 */
id: string, 
/**
 * What the menu says, e.g. "VS Code".
 */
name: string, 
/**
 * The command that opens the checkout.
 */
command: string, 
/**
 * The command that opens a file, if the application has one.
 */
file_command: string | null, };

/**
 * An application as a person typed it in the dialog: a new one has no id
 * yet.
 */
export type CheckoutApplicationDraft = { 
/**
 * The id it already has; `None` or blank for a new one.
 */
id: string | null, name: string, command: string, 
/**
 * Blank is no file command.
 */
file_command: string | null, };

/**
 * How a checkout was arrived at, or that it was not.
 *
 * - `override`: A path this repository's override carries. Set by hand, and it
 *   wins.
 * - `scan`: A clone the scan matched under the clones root.
 * - `nothing`: Neither -- *no checkout*, with the clone command to copy.
 *
 * Issue #499. `nothing` is a state, not a failure.
 */
export type CheckoutFoundBy = "override" | "scan" | "nothing";

/**
 * What Settings › Checkouts draws, and what every *Open in…* menu reads.
 *
 * Issue #844, R-13a.3.
 */
export type CheckoutSettings = { clones_root: string | null, git: GitFound, 
/**
 * The default's id; `None` only when there are no applications.
 */
default_application: string | null, 
/**
 * In the order the person put them.
 */
applications: Array<CheckoutApplication>, };

/**
 * What a repo or branch detail draws for its checkout.
 *
 * Issue #499. A **checkout** (`CONTEXT.md`) is knobas-owned data about the
 * local disk, never a field of the mirrored repo: knobas issues no clone, no
 * checkout and no fetch from this view (ADR-0016).
 */
export type CheckoutView = { 
/**
 * The repo entity the answer belongs to: the entity asked about when it
 * is a repo, its repository when it is a branch, and `None` when a branch
 * has no repo in the mirror.
 *
 * The frontend needs it to *set* an override, which is keyed on the
 * repository and never on the branch -- a per-branch path would be a
 * second answer to one question and would go stale the moment somebody
 * switched branches in that tree.
 */
repo_entity_id: string | null, 
/**
 * The repository's own stored URL, as the mirror holds it. `None` where
 * the adapter reported no page for it, which is the same P5 miss
 * *Open in browser* answers by being absent.
 */
repo_url: string | null, 
/**
 * Where the clone is, if there is one.
 */
path: string | null, found_by: CheckoutFoundBy, 
/**
 * The configured clones root, so the panel can say *no clones root set*
 * rather than *no checkout* when that is the actual state.
 */
clones_root: string | null, 
/**
 * `git clone <the repo's URL>` -- **text to copy, never run** (ADR-0016).
 * `None` exactly when [`repo_url`](Self::repo_url) is.
 */
clone_command: string | null, 
/**
 * Where the working tree at [`path`](Self::path) stands, read by
 * read-only git when the view is asked for (ADR-0021). `None` exactly
 * when there is no path.
 *
 * Issue #704.
 */
tree: TreeRead | null, };

/**
 * One option of a [`FieldKind::Choice`]: the `value` the op takes, and the
 * `label` the reader sees.
 */
export type ChoiceOption = { 
/**
 * What the op takes when this option is picked (`"squash"`).
 */
value: string, 
/**
 * What the reader sees (`"Squash"`).
 */
label: string, };

/**
 * What `clone_repo` answers.
 *
 * - `cloning`: The clone started, or was already running; events follow.
 * - `adopted`: The folder was already a clone of this repository; nothing ran.
 * - `stopped`: A running clone was stopped. `removed` says whether the
 *   half-made folder is gone.
 * - `not_cloning`: A stop was asked for and no clone of this repository was
 *   running.
 */
export type CloneAnswer = { "status": "cloning", path: string, } | { "status": "adopted", path: string, } | { "status": "stopped", path: string, removed: boolean, } | { "status": "not_cloning" };

/**
 * What one clone reports while it runs and when it ends.
 *
 * The `clone:progress` payload (#823): the repo entity being cloned, the
 * folder, and its state.
 */
export type CloneEvent = { 
/**
 * The repo entity being cloned (a branch's repository when a branch was
 * asked about).
 */
entity_id: string, 
/**
 * The folder it is cloning into.
 */
path: string, } & ({ "state": "progress", phase: string, percent: number, } | { "state": "done" } | { "state": "failed", reason: string, cause: CloneFailureCause, removed: boolean, } | { "state": "stopped", removed: boolean, });

/**
 * [`clone::FailureCause`], on the wire.
 *
 * - `auth`: git could not sign in with the person's own credentials.
 * - `disk_full`: The disk is full.
 * - `other`: The network, the server, or anything else; the reason says which.
 *
 * Why a clone failed (#823).
 */
export type CloneFailureCause = "auth" | "disk_full" | "other";

/**
 * Where a clone is.
 *
 * - `progress`: Running: git's step, and the whole clone from 0 to 100.
 * - `done`: Finished; the scan now finds the folder.
 * - `failed`: git failed. `reason` is git's own line
 *   (`clone::failure_reason`); `removed` says whether the half-made folder is
 *   gone.
 * - `stopped`: Stopped by [`stop`]. `removed` says whether the folder is gone.
 *
 * Issue #823.
 */
export type CloneState = { "state": "progress", phase: string, percent: number, } | { "state": "done" } | { "state": "failed", reason: string, cause: CloneFailureCause, removed: boolean, } | { "state": "stopped", removed: boolean, };

/**
 * What Code draws beside one mirrored row (#966, #973, #986): a pull
 * request's, branch's or commit's author, whether it is mine, and a
 * repository's language and teams.
 *
 * Not on [`EntityRow`]: a room line and a launcher row are one wire shape
 * over two structs (`commands::search`'s pin), read by a dozen statements,
 * and these four facts are Code's alone. Code asks for the rows it holds.
 */
export type CodeFacts = { 
/**
 * The row asked about.
 */
entity_id: string, 
/**
 * The row's §4.1 author, as its source spells the account: Gitea's
 * login of a pull request's opener, of a commit's author, of a branch
 * tip's author. `None` where the source named none.
 */
author: string | null, 
/**
 * Whether `author` is its own source's configured username, trimmed,
 * compared as the attention read compares it (#712). `false` for a
 * source with no username, whose rows are nobody's.
 */
mine: boolean, 
/**
 * A repository's declared language; `None` on a miss (Gitea's `""`).
 */
language: string | null, 
/**
 * A repository's declared teams, in the record's order; empty on a miss.
 */
teams: Array<string>, };

/**
 * How a reader comments on a [`Block::Diff`]'s lines (issue #709).
 */
export type Commenting = { 
/**
 * The [`WriteOp`](crate::WriteOp) identifier the comments are sent
 * through (`submit_review`). The shell offers to comment only where the
 * source's descriptor declares it and the shell can queue it, as for an
 * action.
 */
write_op: string, 
/**
 * The identifier a reply to one of the diff's threads goes through
 * (`reply_to_thread`, #2277), on the threads that say they take one
 * ([`LineThread::replies`](crate::diff::LineThread::replies)); `None`
 * offers no reply. Absent on the wire reads as `None`.
 */
reply: string | null, 
/**
 * The identifier resolving or reopening a thread goes through
 * (`resolve_thread`, #2277), on every thread with an id; `None` where
 * the source cannot resolve threads (Forgejo 17 has no endpoint for it).
 * Absent on the wire reads as `None`.
 */
resolve: string | null, };

/**
 * What kind of working set this is.
 *
 * Stored as lowercase text in `knobas.context.kind`, whose
 * `context_kind_chk` (migration 0010) allows exactly these spellings --
 * and `ALL` is what the test that pins the two together walks.
 *
 * - `epic`: A promoted epic: members start from its tickets.
 * - `ticket`: A promoted ticket: focused on the one piece of work.
 * - `adhoc`: A label the user minted; no source system behind it.
 */
export type ContextKind = "epic" | "ticket" | "adhoc";

/**
 * One context, as `knobas.context` holds it.
 *
 * Issue #47.
 */
export type ContextRow = { 
/**
 * `ctx:<uuid>` -- a local id (`RESERVED_NAMESPACES`), and also the id of
 * this context's `knobas.entity` row.
 */
id: string, kind: ContextKind, title: string, 
/**
 * The promoted entity this context is about; `None` for an ad-hoc label.
 */
anchor_id: string | null, 
/**
 * RFC 3339 on the wire.
 */
created_at: string, 
/**
 * RFC 3339 on the wire, or `None` while the context is live.
 */
archived_at: string | null, };

/**
 * One step of a [`Block::Tree`]'s or [`Block::File`]'s trail: a
 * directory's (or the file's own) name and the section that opens it.
 *
 * Issue #711. A section a block names opens **in the tab it is drawn in**,
 * read with `entity_view(entity, section)`; the id is the source's and never
 * parsed.
 */
export type Crumb = { name: string, section: string, };

/**
 * What happened to one file.
 *
 * - `renamed`: Moved, with or without edits; `old_path` names where from.
 * - `binary`: A binary file changed (or was added or deleted): no lines to
 *   draw.
 */
export type DiffChange = "added" | "modified" | "deleted" | "renamed" | "binary";

/**
 * One file of a diff.
 */
export type DiffFile = { 
/**
 * Where the file is after the change (where it was, for a deletion).
 */
path: string, 
/**
 * Where it was before, for a rename or a copy; `None` otherwise.
 */
old_path: string | null, change: DiffChange, 
/**
 * Added lines, counted over the whole file even where it was cut.
 */
additions: number, 
/**
 * Removed lines, counted the same way.
 */
deletions: number, hunks: Array<Hunk>, 
/**
 * Some of this file's lines were cut by the ceiling.
 */
truncated: boolean, 
/**
 * How alike the two sides of a rename or copy are, in percent, where
 * git says (`similarity index 92%`; #2268).
 */
similarity?: number, 
/**
 * The file's mode before and after, where it changed (`old mode
 * 100644` / `new mode 100755`; #2268): an executable bit set or cleared.
 */
old_mode?: string, new_mode?: string, 
/**
 * The review conversations on this file's lines (issue #709): what a
 * source that keeps line comments read for it, in the order the file
 * reads -- empty from the parser, which knows only the text. Absent on
 * the wire reads as empty, so a diff drawn before #709 decodes.
 *
 * The outdated ones last.
 */
threads: Array<LineThread>, 
/**
 * The blob the file holds after the change, as the diff's `index` line
 * names it (abbreviated as the server printed it; the blob before, for a
 * deletion): what *Viewed* is kept by (#2276), so a file whose content
 * did not change stays viewed through a new commit. `None` where the
 * diff names none.
 */
blob?: string, };

/**
 * One line of a hunk, with its number on each side it exists on.
 */
export type DiffLine = { kind: "context" | "add" | "del", 
/**
 * The line's number before the change; `None` for an added line.
 */
old: number | null, 
/**
 * The line's number after the change; `None` for a removed line.
 */
new: number | null, 
/**
 * The line without its `+`, `-` or ` ` marker.
 */
text: string, };

/**
 * The answer the *Local changes* tab draws: the diff, or why there is none.
 * The same four arms as the working tree's
 * [`TreeRead`](super::working_tree::TreeRead).
 *
 * - `read`: git answered.
 * - `no_git`: No `git` on `PATH`.
 * - `not_a_repo`: The path is not a git working tree, or not there at all.
 * - `failed`: git ran and did not answer: it timed out, or refused with
 *   `reason`.
 *
 * Issue #824. None of the arms is an error.
 */
export type DiffRead = { "status": "read" } & UncommittedDiff | { "status": "no_git" } | { "status": "not_a_repo" } | { "status": "failed", reason: string, };

/**
 * One line of one list: what it is about, and why it is here.
 *
 * Story 59 -- *"every digest line links to the commit, PR, worklog, comment
 * or transition it came from, so that a line is something I can check"* -- is
 * what the first three fields are for, and [`reason`](Self::reason) is what
 * the rest of the row is for. The reason is produced **here**, by the rule
 * that made the line, and not re-rendered from [`verb`](Self::verb) at
 * display time: the discipline `knobas_core::inbox::InboxItem::reason` and
 * `suggest::SuggestionEntry::reason` both record, and for the same reason --
 * a line whose provenance cannot be shown is not shippable.
 *
 * Issue #288.
 */
export type DigestLine = { 
/**
 * The item to open, as an entity id -- what the view addresses.
 *
 * `None` for exactly one line: a running timer on an **ad-hoc label**,
 * which has no item behind it (`CONTEXT.md`'s timer target). The inbox's
 * credential expiry carries the same absence for the same reason -- a
 * line that pointed somewhere wrong would be worse than one that says it
 * has nowhere to point.
 *
 * The view draws that line as text rather than as a link: "DB config for
 * the migration" is a legal thing to be working on, and a digest that
 * dropped it would leave the reader's own afternoon out of their standup.
 */
entity_id: string | null, 
/**
 * The entity's kind, for the monogram. `None` with `entity_id`.
 */
kind: string | null, 
/**
 * What the line is about, in the source's own words -- or the ad-hoc
 * label, for the one line that has no entity. Untrusted text: render it
 * as text.
 *
 * Never as markup (gotcha 7).
 */
title: string, 
/**
 * **Which source.** A `source_config.id` for anything the mirror saw or a
 * write was addressed to; [`KNOBAS`] for a line that is knobas' own
 * record of an act -- a worklog copy, a running timer -- and has no
 * source behind it.
 */
source: string, 
/**
 * **Which verb**, as one word: [`ATTRIBUTED`] for the mirror half,
 * [`LOG_WORK`], [`TIMER`], [`BLOCKED_STATUS`], [`BLOCKED_BY`] -- or, for
 * a write, **the write queue's own `op`**.
 *
 * A `String` and deliberately not an enum, which is where this differs
 * from its near-twin `knobas_core::inbox::InboxItem::category`. That
 * vocabulary is closed and this one is not: a `WriteOp` identifier is the
 * *adapter's* (ADR-0006 grows the set, and an out-of-process adapter can
 * name one this binary has never heard of), so an enum here would need an
 * `Other(String)` arm and would then be a closed type pretending. The
 * five knobas-owned words are consts above so that no reader has to
 * spell one twice.
 *
 * On the wire: `attributed`, `log_work`, `timer`, `blocked_status`,
 * `blocked_by`, or a write's op (`comment`, `transition`, …). A view that
 * branches on it must have a default arm.
 *
 * `attributed`, not `authored`: interfaces §4.1's `author` is whoever the
 * **adapter** says a record belongs to, and Jira's says the assignee, so a
 * mirror line means "the source attributes this to you", never "you wrote
 * it".
 */
verb: string, 
/**
 * Why this line is here, as a sentence naming the source and the verb.
 */
reason: string, 
/**
 * When it happened -- and for a blocker, when the item last moved, which
 * is what the list is ordered by.
 *
 * RFC 3339 on the wire.
 */
at: string, };

/**
 * One step of a page's parent chain.
 */
export type DocAncestor = { title: string, entity_id: string | null, };

/**
 * A page that links to the one being read.
 */
export type DocBacklink = { entity_id: string, title: string, space_key: string | null, };

/**
 * One block of a page body. The fixed set R-10.11 names, and nothing else.
 *
 * Internally tagged (`{"type": "heading", ...}`) so the TypeScript mirror is
 * a discriminated union on `type`.
 *
 * - `heading`: A section heading. `level` is 1 to 6 as the source wrote it;
 *   the reader draws every level as a section head.
 * - `list`: A list. A nested list's items are drawn as a list inside their
 *   item.
 * - `code`: A code block, verbatim, with the language the source named.
 * - `table`: A table, row by row; a header cell says so.
 * - `panel`: An info, note, warning or tip panel, with the blocks inside it.
 * - `placeholder`: Something the source has that the reader does not draw: the
 *   source's own name for it (a Confluence macro's `ac:name`). Never dropped,
 *   so a page with a part knobas cannot draw says so (story 134).
 *
 * Source text, never markup.
 */
export type DocBlock = { "type": "heading", level: number, text: Array<DocInline>, } | { "type": "paragraph", text: Array<DocInline>, } | { "type": "list", ordered: boolean, items: Array<Array<DocBlock>>, } | { "type": "code", code: string, language: string | null, } | { "type": "table", rows: Array<Array<TableCell>>, } | { "type": "panel", kind: PanelKind, title: string | null, blocks: Array<DocBlock>, } | { "type": "placeholder", name: string, };

/**
 * One comment on a page.
 */
export type DocComment = { 
/**
 * The source's id for the comment, where it names one: Confluence's
 * content id, the id its comment write answered (#2074). Docs drops a
 * queued comment by it once the read holds it, whatever the words say by
 * then. `None` from a source whose comments carry no id.
 */
id: string | null, 
/**
 * The author's account name, the spelling `@me` matches. `None` where the
 * record names nobody.
 */
author: string | null, 
/**
 * The author's display name, where the record carries one.
 */
author_name: string | null, 
/**
 * When it was written, RFC 3339, as the source wrote it.
 */
at: string | null, blocks: Array<DocBlock>, };

/**
 * What *Export as Markdown* wrote.
 *
 * Issue #861.
 */
export type DocExport = { 
/**
 * The file's name, `page_file_name(title)`.
 */
file: string, 
/**
 * The folder it is in, absolute.
 */
folder: string, };

/**
 * A page that needs the reader, and who asked.
 */
export type DocForYou = { entity_id: string, title: string, space_key: string | null, 
/**
 * The display name of who mentioned the reader: the newest comment that
 * does, else the page's editor when the body does. `None` where the
 * record does not say (a mention typed as a plain `@name`).
 */
by_name: string | null, };

/**
 * One page the Docs search found.
 */
export type DocHit = { entity_id: string, title: string, space: DocSpaceRef | null, 
/**
 * The parent chain as one line (`Engineering › Payments`), `None` at the
 * top of a space.
 */
path: string | null, 
/**
 * The best sentence, cut around its first match.
 */
snippet: DocSnippet, 
/**
 * How often the needles occur in the title and the indexed text.
 */
matches: number, editor: string | null, editor_name: string | null, updated_at: string | null, };

/**
 * One run of text inside a block.
 *
 * - `text`: Words, with the marks the source put on them.
 * - `link`: A link out of the docs source, to a web address.
 * - `page_link`: A link to another page of the docs source, by its title and
 *   space. `entity_id` is the mirrored page it names, filled by knobas when
 *   the reader is read; the adapter leaves it `None`.
 * - `issue`: A ticket reference (a Confluence Jira-issue macro), drawn as a
 *   live chip. The adapter fills `key`; knobas fills the rest from the mirror
 *   when the reader is read, and leaves them `None` for a ticket it does not
 *   hold.
 * - `mention`: A mention of a person. `user` is the account name where the
 *   source says it (the spelling `@me` matches), `name` the display name where
 *   it is known. `me` is filled by knobas when the reader is read.
 * - `break`: A hard line break inside a block.
 */
export type DocInline = { "type": "text", text: string, strong: boolean, emphasis: boolean, code: boolean, } | { "type": "link", text: string, href: string, } | { "type": "page_link", text: string, title: string, space: string | null, entity_id: string | null, } | { "type": "issue", key: string, entity_id: string | null, title: string | null, status: string | null, } | { "type": "mention", user: string | null, name: string | null, me: boolean, } | { "type": "break" };

/**
 * One page in a space's tree.
 */
export type DocNode = { entity_id: string, title: string, 
/**
 * The page above it, where its declared parent is a mirrored page of the
 * same space; `None` for a page at the top of what the mirror holds.
 */
parent: string | null, };

/**
 * One page, as the reader draws it.
 */
export type DocPage = { entity_id: string, source_id: string, 
/**
 * The source's configured name, falling back to its id: what *Open in …*
 * and the placeholder name (R-10.18 keeps those two source-specific).
 */
source_name: string, title: string, space: DocSpaceRef | null, 
/**
 * The parent chain, outermost first: each ancestor's title, and the
 * mirrored page it is where the mirror holds it.
 */
ancestors: Array<DocAncestor>, 
/**
 * Who made this version (contract §4.1's `author`).
 *
 * In the source's account spelling.
 */
editor: string | null, 
/**
 * The editor's display name, where the source's record carries one.
 */
editor_name: string | null, updated_at: string | null, 
/**
 * The page in the source (*Open in Confluence*).
 */
web_url: string | null, 
/**
 * Whether the source's adapter projects its pages. `false` draws the
 * header and says the body cannot be read; every field below is empty.
 */
readable: boolean, version: number | null, labels: Array<string>, blocks: Array<DocBlock>, comments: Array<DocComment>, 
/**
 * The comment writes to this page that settled as sent with an id the
 * source named for the comment, oldest first (#2074). Docs drops a
 * queued comment once [`DocPage::comments`] holds a comment of that id,
 * whatever its words say by then; a mark whose write is not here falls
 * back to the words.
 */
sent_comments: Array<DocSentComment>, 
/**
 * The identity `@me` resolves to, so the reader can say *You* for the
 * editor and a comment's author.
 */
me: Array<string>, 
/**
 * The page's sections, numbered as the anchors are, each with its text
 * or why knobas does not edit it (#862).
 */
sections: Array<DocSection>, 
/**
 * The versions the record names, newest first (#861).
 */
history: Array<DocVersion>, 
/**
 * The page's backlinks: the live pages of the same source whose body
 * links to this one, by title (#861, *Linked from*).
 */
backlinks: Array<DocBacklink>, 
/**
 * The tickets the page references or is linked to in knobas, with their
 * status now (#861, *Work it belongs to*).
 */
work: Array<DocWork>, 
/**
 * Which Docs writes this page's source takes: `comment`, `update_page`,
 * `create_page`, each where its adapter declares it and (for the two
 * page writes) writes the source's markup (#862).
 */
writes: Array<DocWrite>, };

/**
 * One section of a page body: a top-level heading and what follows it up to
 * the next one (#862).
 */
export type DocSection = { 
/**
 * 1 for the first top-level heading.
 */
number: number, 
/**
 * The section's body as the plain text the editor opens with (`- ` for
 * a list item, a blank line between paragraphs, and a panel as a quote
 * whose first line is its kind in bold, `> **Info** <title>`, #975),
 * `None` when it is refused.
 */
text: string | null, 
/**
 * Why knobas does not edit it, `None` when it does.
 */
refusal: SectionRefusal | null, };

/**
 * A comment write knobas sent to a page, and the id the source gave the
 * comment (`knobas.write_queue.remote_id`, stamped by the settle from the
 * write's receipt): the pair a queued comment is matched to its comment by.
 */
export type DocSentComment = { 
/**
 * The queued write's id, the one `submit_write` answered.
 */
write_id: number, 
/**
 * The source's id for the comment it made: [`DocComment::id`].
 */
comment_id: string, };

/**
 * A hit's snippet: where it is, and its text with the matches marked.
 *
 * Source text, never markup.
 */
export type DocSnippet = { 
/**
 * The section heading it sits under, `None` before the first one.
 */
section: string | null, 
/**
 * The comment's author's display name when it is a comment's; `Some("")`
 * for a comment whose record named nobody.
 */
comment_by: string | null, segments: Array<{ text: string, hit: boolean, }>, };

/**
 * One space and its pages.
 */
export type DocSpace = { source_id: string, key: string, 
/**
 * The space's name, `None` where no page of it carries one knobas can
 * read.
 */
name: string | null, 
/**
 * Every live page of the space, in the source's sibling order where it
 * gives one, else by title (#953).
 */
pages: Array<DocNode>, };

/**
 * A space, named.
 */
export type DocSpaceRef = { key: string, name: string | null, };

/**
 * One version of a page, as the record names it.
 */
export type DocVersion = { number: number | null, 
/**
 * The account that made it.
 */
by: string | null, by_name: string | null, 
/**
 * When, RFC 3339, as the source wrote it.
 */
at: string | null, 
/**
 * The version's message, where the editor left one.
 */
message: string | null, };

/**
 * A ticket a page belongs to.
 */
export type DocWork = { entity_id: string, 
/**
 * The entity id past its namespace (`PAY-231`): the job's address.
 */
key: string, title: string, status: string | null, 
/**
 * Whether a knobas link draws it (else the body references it).
 */
linked: boolean, };

/**
 * [`DOC_WRITES`] as the generated TypeScript's `DocWrite` union (#1326): a
 * Docs write a page's source takes, one entry of [`DocPage::writes`]. Never
 * built: the wire carries the op identifier as a string, and this enum only
 * names the union. `the_doc_write_union_is_the_list` pins it to the list.
 */
export type DocWrite = "comment" | "update_page" | "create_page";

/**
 * The Docs search's answer (#860, R-10.3).
 */
export type DocsHits = { 
/**
 * Every page the query admits, before the limit.
 */
total: number, took_ms: number, 
/**
 * The words and phrases the reader marks, lowercased: each word, each
 * quoted phrase whole, never one left out.
 */
needles: Array<string>, 
/**
 * What the grammar could not read.
 */
unknown_tokens: Array<string>, 
/**
 * The best pages, best first.
 */
hits: Array<DocHit>, };

/**
 * Every space the mirror holds pages of, and the page read last.
 */
export type DocsTree = { 
/**
 * Ordered by source, then key.
 */
spaces: Array<DocSpace>, 
/**
 * The page the reader opened most recently (`visit_entity`), while it is
 * still a live page; `#/docs` opens it.
 */
last_read: string | null, 
/**
 * *For you* (#860, R-10.2): the pages whose mention of the reader is in
 * the inbox now, newest first.
 */
for_you: Array<DocForYou>, };

/**
 * What one download streams (#854's *Download log*, #856's downloads).
 * Tagged on `kind`.
 *
 * - `log`: The run's whole log.
 * - `file`: One artifact file.
 * - `folder`: One artifact folder, as a zip.
 * - `all`: Every artifact, as a zip.
 */
export type DownloadTarget = { "kind": "log" } | { "kind": "file", path: string, } | { "kind": "folder", path: string, } | { "kind": "all" };

/**
 * Everything the slide-over draws for one entity (interfaces §2.5).
 */
export type EntityDetail = { row: EntityRow, source: SourceRef, 
/**
 * The adapter's own label and monogram for this kind.
 *
 * Resolved from the adapter registry by the row's `adapter_kind` -- see
 * [`kind_info_for`]. `None` for a kind the adapter does not declare, and
 * for a source with no configuration row at all (which `run_once`
 * produces). Both cases fall through to the frontend's §3a humaniser,
 * which is what §3a asks for when nothing declares anything.
 */
kind_info: KindInfo | null, 
/**
 * Whether the item's adapter reads one item **of its kind** by id
 * (#2107, #2146, ADR-0031): its descriptor declares `Capability::Refresh`
 * and the kind's `KindInfo::refreshes`. The detail draws
 * *Refresh* where it does and *Sync now* where it does not, both only
 * while [`SourceRef::enabled`]. Off the adapter's template, so the read
 * builds no source; `false` for a source with no configuration row or
 * an adapter not installed. An instance that answers *not offered* all
 * the same gets the run in its place.
 */
refreshes: boolean, 
/**
 * Untrusted source text. Rendered as text, never as markup (gotcha 7).
 */
body_text: string, author: string | null, 
/**
 * The source record **verbatim** (§3a): what the generic detail view
 * projects. It is untrusted text all the way down.
 *
 * `unknown` in the shell, per §2.6's `serde_json::Value` mapping: project
 * it as text, never as markup (interfaces §2.5, gotcha 7).
 */
payload: unknown, 
/**
 * Where the item lives in its own system, or `None` when the adapter
 * reported no page -- in which case *Open in browser* is absent (P5).
 */
web_url: string | null, 
/**
 * Set when the source withdrew the entity. The mirror row survives, so
 * links and notes still resolve (§5a).
 *
 * RFC 3339 on the wire, `None` while the source still has it.
 */
deleted_at: string | null, 
/**
 * Every link this entity takes part in, newest first, each with the end
 * the reader is *not* on already resolved.
 *
 * Read undirected by [`knobas_core::link::entries_of`], so a link drawn
 * from either end is on both ends' detail, and hydrated there, so the
 * panel draws a kind and a title rather than a raw id -- including for a
 * target the source withdrew. [`create_link`] and [`unlink`] are what
 * move it.
 */
links: Array<LinkEntry>, 
/**
 * This entity's own history, newest first (spec §12.1).
 */
activity: Array<ActivityRow>, 
/**
 * The mirrored builds that ran on the commit this entity stands at
 * (#710): for a pull request its head commit, for a branch its tip, for
 * a commit itself -- whatever its kind declares as `commit` -- the rows
 * whose declared `revision` names that sha. Failing first, then running,
 * then the rest by tone, newest first within one; at most
 * [`DETAIL_BUILDS`]. Empty for a kind that declares no commit, which is
 * every kind but code, and for a commit nothing mirrored ran on.
 */
builds: Array<EntityRow>, 
/**
 * A ticket's status as its source declares it (#760), read once beside
 * the row by the rule the mini board reads it by
 * ([`declared_string!`](knobas_core::declared_string)); the ticket detail's
 * select shows it as the ticket's current one.
 *
 * `None` for anything that is not a ticket, for a ticket whose source
 * declares no status or whose record does not carry it (the board's
 * terminal group, "No status"), and for a withdrawn ticket or a disabled
 * source's, which the board does not draw either.
 */
status: string | null, 
/**
 * A ticket's type in its source's own word (#816, `Story`, `Bug`), read
 * through the declared `issue_type` path; the job card draws it. `None`
 * where nothing is declared or the record misses it (ADR-0007).
 */
issue_type: string | null, 
/**
 * A ticket's priority in its source's own word (#816), through the
 * declared `priority` path. `None` on a miss.
 */
priority: string | null, 
/**
 * A ticket's original estimate in seconds (#816), through the declared
 * `estimate` path: a whole JSON number or nothing, never zero for a
 * miss. The job card writes it as *est. 3h*.
 */
estimate_seconds: number | null, };

/**
 * Which slice of the corpus a room wants.
 *
 * Both lists are *unfiltered when empty*, which is why they are bound as
 * `NULL` rather than as an empty array: `= any('{}')` matches nothing, and a
 * room whose filter said "no sources in particular" would come back empty.
 * Do not "helpfully" send every known id instead of an empty list.
 *
 * Deserialize only: a filter is something the frontend *sends*, and its
 * TypeScript is generated from this struct (#1326), so a field added here is
 * a field the shell's type has without a round trip to prove it.
 */
export type EntityFilter = { 
/**
 * Source ids to include; empty means every source.
 */
sources: Array<string>, 
/**
 * Kinds to include; empty means every kind.
 */
kinds: Array<string>, 
/**
 * Only items the source dated within this many days. Items the source
 * never dated are excluded by the window, because the window is about the
 * source's own timestamp.
 */
updated_within_days: number | null, 
/**
 * Only members of this stored context (`ctx:<id>`), by the fixed one-hop
 * rule (§16.11, ADR-0008). `None` is unscoped -- every derived room. The
 * membership is resolved server-side per read rather than shipped as an
 * id list, so a page and its `total` always describe the same instant.
 *
 * The fixed rule is the seed, its direct links, one hop, and what the
 * assets among them hold.
 */
context: string | null, 
/**
 * Only items carrying this project key, in the source's own word
 * (ADR-0010, issue #208). `None` is unscoped, the same "empty means
 * unfiltered" the two lists above have.
 *
 * It narrows **within** [`Self::sources`] and never instead of them: a
 * project key is unique only inside its own source, so a project room
 * sets both and a filter carrying the key alone would union two sources'
 * `PAY` into one room. Where a key is read from is the **source's** to
 * say since #277, and it is the same read the census behind the
 * switcher's project rooms takes -- one macro over one declaration, so a
 * room can never disagree with the list of rooms about what a project
 * is.
 *
 * A record whose payload carries no readable project key matches no
 * value of this field, so it is in no project room and still in *All
 * work* and its source's room: absence, never a wrong room.
 */
project: string | null, order: EntityOrder, 
/**
 * Whether to reach past `sync.live_item` for entities withdrawn upstream
 * (§5a: links and notes may point at them).
 */
include_deleted: boolean, 
/**
 * Only what this ticket reaches (#817): the job-reach read
 * (`crate::reach`), resolved server-side per read as
 * [`Self::context`] is. `None` is unscoped. With a context as well, a
 * row must be in both. Absent on the wire reads as `None`, so a caller
 * written before #817 is unscoped as it was.
 */
reach?: string | null, 
/**
 * Only what this role's surface draws (#1294, ADR-0029 as amended on
 * 2026-10-19): the rows of the sources that **fill** the role, which the
 * mirror answers per read (`sources::crud::fillers`). With
 * [`Self::sources`] as well, the fillers among them. With
 * [`Self::kinds`] empty it also narrows to the role's kinds (CI's are
 * the run kinds its fillers declare, #1344: a pipeline is the
 * `pipelines` read's); with kinds named, to the role's among them. An unknown role has no fillers, so its page is
 * empty, never an error. `None` is unscoped, and absent on the wire
 * reads as `None`, as [`Self::reach`] does.
 */
role?: string | null, };

/**
 * The orderings a room reads in: the two plain ones, and since #680 the two
 * tile orders.
 *
 * - `updated_desc`: Newest first, by the source's own timestamp. Undated items
 *   last.
 * - `title_asc`: Alphabetical by title.
 * - `attention`: **What needs attention first** (#680): a room's Code tile. A
 *   row whose declared state is **settled** -- tone `ok` (done and good: a
 *   merged pull request) or `muted` (over: a closed one) -- is not in the set.
 *   What is left is ordered by its state's tone, loudest first (`err`, `warn`,
 *   `info`), then rows with no state, each run newest first.
 *
 *   Driven by the declared state and its tone, never by a kind or an adapter:
 *   a kind whose source declares no state, and a record whose declared state
 *   misses, has no tone, so nothing hides it and it sorts by recency (§3a; the
 *   ADR-0007 miss fails toward drawing the row).
 * - `latest_per_parent`: **One row per parent, its latest** (#680): a room's
 *   Builds tile. Where a row's kind declares a state, its rows are grouped by
 *   the parent chain their kind declares (ADR-0018) -- every name each parent
 *   answers to, so two configurations sharing a display name stay two -- and
 *   only the newest of each group is kept: a configuration's health is its
 *   latest build. The kept rows are ordered by tone, every tone drawn (`err`,
 *   `warn`, `info`, `ok`, `muted`, then no state), each run newest first.
 *
 *   A kind that declares no state is not grouped, and a row whose parent
 *   misses stands alone: both fall back to recency (§3a), so a source that
 *   never said what its builds' outcomes are draws every build.
 *
 * Both tile orders read the live corpus only.
 */
export type EntityOrder = "updated_desc" | "title_asc" | "attention" | "latest_per_parent";

/**
 * One page of a room, plus the size of the set it was cut from.
 */
export type EntityPage = { rows: Array<EntityRow>, 
/**
 * The whole filtered set, before `limit`/`offset`. `0` for an empty page.
 */
total: number, 
/**
 * Under a tile order ([`EntityOrder::Attention`],
 * [`EntityOrder::LatestPerParent`]), the set counted by kind and tone,
 * in no particular order; `None` under the plain orders, whose header
 * is `total` alone (#680).
 */
tally: Array<KindTally> | null, 
/**
 * Under [`EntityOrder::Attention`] in a room where some row's source has
 * a configured identity, the set's non-empty [`AttentionGroup`]s in draw
 * order, each with its count (#712). The rows arrive group by group, so
 * a page read from offset 0 is cut into its groups by these counts.
 * `None` under every other order and in a room nobody is "me" in, which
 * draws as it did before.
 */
groups: Array<GroupTally> | null, };

/**
 * One line in a room: what a tile draws, and nothing more.
 *
 * Deliberately not the mirror row. `body_text` and `payload` are the detail
 * view's business (the app's `EntityDetail`), and a room that fetched them would pull
 * the whole corpus into the webview to render five columns of it.
 */
export type EntityRow = { 
/**
 * `"<namespace>:<key>"` -- `mock:PAY-231`. The kind is not part of it.
 */
entity_id: string, kind: string, source_id: string, 
/**
 * Raw source text. Render it as text, never as markup (gotcha 7).
 */
title: string, 
/**
 * When the *source* says the item changed.
 *
 * `None` where the source never said (interfaces §4.1 normalization):
 * the mirror keeps the hole rather than filling it with `now()`, so a row
 * that has never been dated is distinguishable from one changed this
 * second.
 *
 * RFC 3339 on the wire. The room sorts an undated row last rather than
 * first.
 */
updated_at: string | null, 
/**
 * When knobas last saw it. Always known -- it is knobas' own clock.
 *
 * RFC 3339 on the wire.
 */
synced_at: string, 
/**
 * Where this row sits **inside its source**, as one line -- a Confluence
 * page's ancestor path, `Engineering \u{203a} Payments` (#284, ratified
 * as a contract §10.8 exception under that ticket's criterion 5).
 *
 * Since #596 it is the **parent chain** the row's kind declares
 * (ADR-0018) -- `owner/repo` for a Gitea branch, pull request or commit,
 * a page's ancestors since Confluence declared them (#599) -- and nothing
 * else since #646 retired the undeclared ancestor read.
 *
 * `None` for every row whose kind declares no parent or whose declared
 * parent misses: the ADR-0007 **miss**. Joined by
 * `crate::path_line_read!`, the one expression that spells it, so
 * this row and the launcher's cannot disagree about what a path is.
 *
 * This struct and `knobas_search::EntityRow` are pinned to **one wire
 * shape** by `knobas-app`'s `commands::search` test; the field is on both.
 */
path: string | null, 
/**
 * The item's state chip -- a pull request's `open` / `merged` /
 * `closed`, a build's `running` or outcome (#666) -- resolved by
 * `crate::declared_state!` from the state the row's kind declares,
 * beside the path line and wherever it is read. `None` for a kind that
 * declares none and for a value the declaration does not name: the
 * ADR-0007 **miss**, so a row shows no chip rather than a wrong one.
 *
 * On both `EntityRow`s, pinned to one wire shape like `path`.
 *
 * `text` is source text: render it as text.
 */
state: StateChip | null, };

/**
 * What a [`TreeEntry`] is, as git records it.
 */
export type EntryKind = "dir" | "file" | "symlink" | "submodule";

/**
 * A stretch of one end's text.
 */
export type EvidenceExcerpt = { 
/**
 * The end the text is from.
 */
entity_id: string, 
/**
 * The text in order; the marked parts are the match. Parts rather than
 * offsets, so no reader has to agree with Rust on what a character is.
 */
parts: Array<EvidencePart>, };

/**
 * One run of an excerpt's text.
 */
export type EvidencePart = { text: string, 
/**
 * The match: the key, the host or a shared word.
 */
marked: boolean, };

/**
 * One label and its value.
 */
export type Fact = { label: string, value: string, 
/**
 * Colour, only where something is wrong or moving (the Signal design).
 */
tone: Tone | null, };

/**
 * How an [`ActionField`] asks. Internally tagged (`{"type": "choice", ...}`),
 * a union on `type` in the TypeScript mirror as [`Block`] is.
 *
 * - `text`: One line. `suggestions` are offered as the reader types and none
 *   is picked: a run's branch suggests the branches recent builds ran on.
 * - `long_text`: Several lines of Markdown: a description, a comment. Drawn as
 *   a plain text area; the shell never renders it (ADR-0020 rule 5).
 * - `choice`: One of `options`, picked.
 * - `flag`: A checkbox.
 */
export type FieldKind = { "type": "text", suggestions: Array<string>, } | { "type": "long_text" } | { "type": "choice", options: Array<ChoiceOption>, } | { "type": "flag" };

/**
 * A [`LineThread`] on a file the diff no longer holds (issue #709): written
 * on `path` in an earlier round, which a later push reverted or dropped.
 * Outdated by definition -- there is no line left to be on -- and drawn in
 * the diff's own outdated list, never dropped.
 */
export type FileThread = { path: string, thread: LineThread, };

/**
 * The `git` knobas runs at a checkout, as Settings › Checkouts says it.
 *
 * Where it is and nothing more: knobas runs git only for `status`, `diff`
 * and `clone` (ADR-0021, ADR-0026, ADR-0023), so it does not ask git its
 * version.
 *
 * - `found`: The `git` on `PATH`.
 * - `no_git`: None, or Apple's shim without the Command Line Tools.
 * - `failed`: The Command Line Tools probe did not answer.
 *
 * Issue #844.
 */
export type GitFound = { "status": "found", path: string, } | { "status": "no_git" } | { "status": "failed", reason: string, };

/**
 * How many rows of the set one [`AttentionGroup`] holds.
 */
export type GroupTally = { group: AttentionGroup, count: number, };

/**
 * One `@@` hunk.
 */
export type Hunk = { 
/**
 * The `@@ -a,b +c,d @@ context` line as the server sent it.
 */
header: string, lines: Array<DiffLine>, };

/**
 * The seven kinds of demand the inbox distinguishes.
 *
 * Serialized as its stored spelling, which is also the first half of every
 * [`InboxItem::key`]. **No CHECK pins it**: no column holds a category,
 * because no table holds an item -- `knobas.inbox_state` stores the composed
 * key as opaque text and nothing constrains it. It is declared with
 * [`crate::closed_vocabulary!`] all the same (#1989), for the rest of what the
 * macro gives: `ALL` and `as_str` from the variant list, so a category cannot
 * be missing from `ALL`, and the codec the stream's statement decodes its
 * computed `category` column through.
 *
 * - `review_request`: Somebody is waiting on your review of a pull request.
 * - `mention`: Somebody wrote your name at you.
 * - `failed_build`: A build on your work went red.
 * - `new_assignment`: Work arrived: a ticket somebody else pointed at you.
 * - `credential_expiry`: A credential knobas holds stops working soon.
 * - `failed_sign_in`: A source refuses knobas' sign-in, or knobas holds no
 *   credential for it, so it takes nothing until somebody acts.
 * - `alert`: A monitor watching something in the estate is down or slow, and a
 *   context holds the asset it watches.
 *
 * The shell's union is generated from this enum, so a category added here
 * makes a total `switch` over it fail `svelte-check` rather than fall through
 * to a label nobody notices.
 */
export type InboxCategory = "review_request" | "mention" | "failed_build" | "new_assignment" | "credential_expiry" | "failed_sign_in" | "alert";

/**
 * One line of the stream, with the actions this source can really perform.
 *
 * **Nested, not flattened** -- `{item, actions}` -- which is #53's ratified
 * shape for exactly this pairing and is recorded there as a decision rather
 * than a habit: the item is `knobas_core`'s record and the actions are this
 * crate's answer about it, and a flattened bag would make a later reader
 * guess which half a field came from.
 */
export type InboxEntry = { item: InboxItem, 
/**
 * `knobas_source::WriteOp` identifiers, in the order the category asks
 * for them, filtered to what this item's source declares.
 *
 * Empty is a real answer and not a missing one: a credential expiry has
 * nothing to ask a source for, and a source with no write capability
 * offers nothing on any of its items. *Open*, *snooze* and *done* are not
 * in here -- they are knobas' own and always available.
 *
 * Best first; dispatch one with `submit_write`. **Render exactly these and
 * no others**: an op absent from this list is one the source cannot
 * perform, and a button for it would fail on press.
 */
actions: Array<string>, };

/**
 * One line of the stream.
 *
 * Everything a reader needs to judge an item without opening it (#45, story
 * 7): which source it came from, what it is about, and why it is here.
 *
 * **Derived, not stored.** There is no inbox table: every field here is
 * computed from the mirror when the stream is read, which is why an item
 * leaves on its own when the thing it was about is resolved at the source.
 */
export type InboxItem = { 
/**
 * `'<category>:<subject>'` -- what snooze and done are keyed on, and what
 * the interface hands back when the user answers.
 *
 * Stable across syncs: the category is a fixed word and the subject is an
 * entity id (or a source id, for a credential expiry or a failed
 * sign-in). The shell never
 * composes one: it hands back the string it was given, to
 * `snooze_inbox_item` and `complete_inbox_item`.
 */
key: string, 
/**
 * Which demand this is. Decoded from the statement's text column so the
 * wire form is the enum rather than a bare string.
 */
category: InboxCategory, 
/**
 * The source that owes this item -- a `source_config.id`, which is also
 * the entity namespace.
 */
source_id: string, 
/**
 * The entity to open. `None` only for credential expiry and a failed
 * sign-in, whose subject is a source and not an entity: the inbox is a
 * way in, not a dead end (story 8), and an item with no entity behind it
 * says so by carrying none rather than by pointing somewhere wrong.
 *
 * An item with no entity has no *Open*, and that is honest rather than
 * incomplete.
 */
entity_id: string | null, 
/**
 * The entity's kind, for the monogram. `None` with `entity_id`.
 */
kind: string | null, 
/**
 * What the item is about, in the source's own words.
 *
 * Raw source text. Render it as text, never as markup (gotcha 7).
 */
title: string, 
/**
 * Why this is in the stream, as a sentence -- produced by the rule, not
 * re-rendered from the category at display time. The same discipline
 * `suggest::SuggestionEntry::reason` gets: a demand whose reason cannot
 * be shown is not shippable.
 *
 * Also raw text.
 */
reason: string, 
/**
 * When the item last moved. The stream's ordering, and what *done* is
 * measured against.
 *
 * RFC 3339 on the wire.
 */
occurred_at: string, 
/**
 * Where a human reads this in the source's own UI, when the mirror knows.
 */
web_url: string | null, 
/**
 * When this item comes back, for the rows on the snoozed shelf. `None` on
 * the stream itself.
 *
 * RFC 3339 on the wire.
 */
snoozed_until: string | null, };

/**
 * Which shelf a read wants.
 *
 * Serialized snake_case because it crosses the IPC boundary as a command
 * argument: the inbox view and the snoozed panel are one read with one
 * predicate, and asking for the shelf by name is what keeps them from
 * becoming two statements that can disagree about what "snoozed" means.
 *
 * - `stream`: What needs you now. This is the inbox.
 * - `snoozed`: What you deferred, and when it comes back.
 *
 * One derivation with one predicate, asked for one side or the other, so the
 * two shelves can never overlap or leave a gap.
 */
export type InboxShelf = "stream" | "snoozed";

/**
 * One of a parent ticket's own links: the far end.
 */
export type JobLink = { entity_id: string, kind: string, title: string | null, };

/**
 * What a job shows beside its own links (#867): the sets that apply, by
 * name, and the parent behind R-7.3's switch.
 */
export type JobLinkSets = { 
/**
 * The sets that apply, by name.
 */
sets: Array<JobSet>, 
/**
 * `None` with no parent, an epic for one, or R-7.3's switch off.
 */
parent: JobParent | null, };

/**
 * One member of a set that applies to a job (#867): what the drawer's
 * group lists, and whether *Hide here* hid it on this ticket.
 */
export type JobMember = { 
/**
 * The member's kind, one of [`MEMBER_KINDS`]: the row's section.
 */
kind: LinkSetMemberKind, entity_id: string, 
/**
 * The entity's own kind, what the row links by; `None` when knobas has
 * no entity of that id.
 */
entity_kind: string | null, title: string | null, 
/**
 * Hidden on this ticket by *Hide here* (R-7.5).
 */
hidden: boolean, };

/**
 * The parent whose own links a sub-task shows (R-7.3).
 */
export type JobParent = { 
/**
 * The parent ticket's entity id.
 */
ticket: string, title: string | null, links: Array<JobLink>, };

/**
 * One pipeline on a job, with every reason it is there (#850): branches
 * first, then pull requests, then links, as prototype-6 lists them.
 */
export type JobPipeline = { pipeline_id: string, reasons: Array<PipelineReason>, };

/**
 * One set that applies to a job (R-4.4, R-6.2): the chip's name, the reason
 * (the first of its rules that matches, in the set's order) and its members.
 */
export type JobSet = { id: string, name: string, 
/**
 * The first of its rules that matches the ticket, in the set's order: the
 * reason.
 */
via: LinkSetRuleRow, members: Array<JobMember>, };

/**
 * Display metadata for one entity kind an adapter emits, and whether a full
 * sync of that kind is exhaustive.
 */
export type KindInfo = { 
/**
 * Matches [`SyncItem::kind`], e.g. `"ticket"`.
 */
id: string, 
/**
 * Singular label, e.g. `"Ticket"`.
 */
label: string, 
/**
 * Plural label, e.g. `"Tickets"`.
 */
plural: string, 
/**
 * Two-character monogram for the item chip, e.g. `"JI"`.
 */
monogram: string, 
/**
 * Whether a `cursor: None` sync emits the **complete** current corpus
 * *of this kind*.
 *
 * This is the precondition of the full-sync sweep (interfaces §4.1):
 * after an exhaustive full sync, every row of this source **and this
 * kind** whose `synced_at` predates the run is an item the source stopped
 * returning, and the engine tombstones it -- which is the only way a hard
 * delete upstream ever reaches knobas.
 *
 * `false` says the full sync of this kind is a *window*, not the world:
 * TeamCity emits the newest N builds per configuration, so a sweep after
 * it would tombstone the entire build history on every run. Vanished
 * items of such a kind are never swept, and the mirror keeps what it last
 * saw.
 *
 * **Per kind, not per source** (ADR-0003, ratified 2026-08-27). One
 * adapter routinely walks some kinds exhaustively and budgets others:
 * Gitea enumerates every repository and every branch, but bounds commits
 * and pull requests with `commits_per_repo` / `prs_per_repo`. A single
 * per-source flag forced one answer for all four -- `true` licensed the
 * sweep to tombstone every commit past the cap on every full sync, and
 * `false` left repo-row retirement and branch hard deletes inexpressible.
 *
 * A **budgeted kind is non-exhaustive by definition**; declaring
 * otherwise is the defect class of Jira's `MAX_PAGES` and the 2026-08-25
 * Gitea budget ruling.
 *
 * M2: mock all `true`, Jira `ticket` `true`, Gitea `repo`/`branch`
 * `true` and `commit`/`pr` `false`, TeamCity both `false`. It is a claim
 * the adapter makes about its own read path -- the battery cannot check
 * it without knowing the remote corpus, so the adapter's own integration
 * tests are what hold it honest.
 */
full_sync_exhaustive: boolean, };

/**
 * How many rows of one kind, in one tone, a tile order drew (#680).
 *
 * What a tile's header phrases ("2 open pull requests · 5 branches", "1
 * failing · 7 build configurations"), counted over the whole ordered set
 * rather than the page, as [`EntityPage::total`] is.
 */
export type KindTally = { kind: string, 
/**
 * The tone of the rows' state chip; `None` for rows that have none.
 */
tone: Tone | null, count: number, };

/**
 * One status of a lane: its cards by id, newest first, and its count.
 *
 * Issue #939.
 */
export type LaneColumn = { 
/**
 * As [`MiniBoardColumn::status`].
 *
 * `None` is the terminal group.
 */
status: string | null, 
/**
 * The ids of the cards kept, each in [`MiniBoard::columns`].
 *
 * Newest first, or in the saved view's order.
 */
ids: Array<string>, 
/**
 * How many of the lane's tickets stand in this status, kept or not.
 */
total: number, };

/**
 * What a board's lanes split by (R-3.5): the Tickets view's *Lanes*
 * switch, spelled as a ticket view stores it.
 *
 * - `epic`: The epic: the ticket's path line, as the card draws it.
 * - `issuetype`: The ticket's type.
 * - `label`: The ticket's first label.
 */
export type LaneField = "epic" | "assignee" | "priority" | "issuetype" | "label";

/**
 * One comment of a [`LineThread`]. Source text, like every string of a view:
 * `body` is the source's Markdown, drawn legibly and never as markup
 * (ADR-0020 rule 5).
 */
export type LineComment = { 
/**
 * Who wrote it, as the source names them (a login).
 */
author: string, body: string, 
/**
 * When, as the source's RFC 3339 instant; `None` where it sent none.
 */
at: string | null, };

/**
 * What a [`ViewAction`] does with the reader's pending line comments (issue
 * #709). The comments are the shell's -- written on a commentable
 * [`Block::Diff`] and kept until sent or discarded -- and the action only
 * says whether they go with it.
 *
 * - `none`: The action never sends them (*Comment…*, *Merge…*).
 * - `sends`: The action sends them when there are some, and is drawn either
 *   way (*Approve…*, *Request changes…*): its dialog lists what goes with it.
 * - `requires`: The action sends them and is drawn only while there are some
 *   (*Submit review…*, a review that is the comments and nothing else).
 */
export type LineComments = "none" | "sends" | "requires";

/**
 * One conversation on one line of a file's diff (issue #709, `CONTEXT.md`
 * **Line thread**): the comments a review left there, oldest first.
 *
 * `line` counts on `side`, as a [`ReviewComment`](crate::ReviewComment)'s
 * does: the file as it was (`old`) or as it becomes (`new`). The shell draws
 * the thread under the line holding that number on that side.
 *
 * **Outdated is the source's answer** (ADR-0007): the line is no longer in
 * this diff as it was when the comment was made -- gone, or another text in
 * its place since a later push. An outdated thread is drawn muted at the end
 * of its file, never on a line it was not written about.
 */
export type LineThread = { side: ReviewSide, line: number, outdated: boolean, comments: Array<LineComment>, 
/**
 * The source's id for the thread, which a reply or a resolve names
 * ([`WriteOp::ReplyToThread`](crate::WriteOp::ReplyToThread),
 * [`WriteOp::ResolveThread`](crate::WriteOp::ResolveThread); #2277);
 * `None` where the source offers neither on it. Absent on the wire
 * reads as `None`.
 */
id: string | null, 
/**
 * Who resolved the thread, as the source names them; `None` while it is
 * open (#2277). The shell draws a resolved thread collapsed. Absent on
 * the wire reads as `None`.
 */
resolved_by: string | null, 
/**
 * Whether the source takes a reply on this thread (#2277): Gitea places
 * a reply by line on the current head, so an outdated thread takes none.
 * Absent on the wire reads as `false`.
 */
replies: boolean, };

/**
 * The end of a link that the reader is **not** looking at.
 *
 * A link row carries two ids and nothing else, and an id is not something a
 * person recognises: the panel has to say *what* was linked (spec §5a, #40's
 * story 9). Resolved here rather than by a second read per row, because a
 * detail view drawing ten links would otherwise make ten round trips.
 */
export type LinkEnd = { 
/**
 * `"<namespace>:<key>"`: the address a row navigates to.
 */
entity_id: string, kind: string, 
/**
 * The entity's own title. Untrusted source text: render it as text.
 *
 * Never as markup (gotcha 7).
 */
title: string, 
/**
 * Set when the source withdrew this entity.
 *
 * Not a filter -- a *fact* the reader is shown. A link may point at
 * something that vanished upstream, and hiding it would be a link that
 * dangles silently (#40's story 10).
 *
 * RFC 3339 on the wire. Not a reason to hide the row: §5a keeps the entity
 * so a link never dangles, and the panel marks it instead.
 */
deleted_at: string | null, };

/**
 * One link, as a detail view draws it: the record, and its other end.
 *
 * Which end `other` holds depends on whose detail was read: `A -> B` hydrates
 * `B` on A's panel and `A` on B's. Direction and the inverse-label wording are
 * the frontend's (`detail/relations.ts`), computed from `link.from_id` against
 * the entity being viewed.
 */
export type LinkEntry = { link: LinkRow, 
/**
 * The end that is not the entity this was read for.
 *
 * Which end that is depends on who is reading, which is why this cannot
 * be resolved once and cached on the row: `A -> B` hydrates `B` on A's
 * detail and `A` on B's.
 */
other: LinkEnd, };

/**
 * One active link, as seen from either of its ends.
 */
export type LinkRow = { id: string, from_id: string, to_id: string, 
/**
 * What kind of link this is: `"related"` unless one was named.
 *
 * Always **lower case**: the backend folds it on write, so `Blocks` and
 * `blocks` are one relation and one panel group rather than two. Key the
 * curated list and the inverse-label lookup on this spelling; capitalize
 * for display from the whole string.
 */
relation: string, 
/**
 * Where the link came from:
 *
 * - `manual`: Drawn by the user.
 * - `suggested`: Proposed by knobas and confirmed by the user.
 * - `imported`: Restored from an export.
 * - `source`: Mirrored from a relation the source system already states.
 * - `implied`: Drawn by knobas as a consequence of what the user wrote: a
 *   note's `[[ref]]` links are the population (`crate::note`'s
 *   `reconcile_refs`, which owns every row of this origin).
 */
origin: "manual" | "suggested" | "imported" | "source" | "implied", 
/**
 * Why the link exists, in the user's own words -- `None` unless one was
 * given.
 *
 * The column has been in the schema since `0001` and reached nothing
 * until Links v1 wired it through (#40): store row, DTO, TypeScript
 * mirror. Nullable rather than defaulted to `""`, because "no reason
 * recorded" and "a reason recorded as nothing" are different facts and
 * only one of them is worth a line in the panel.
 */
note: string | null, created_by: string, 
/**
 * RFC 3339 on the wire.
 */
created_at: string, 
/**
 * When the user said yes -- and, by being `None`, the whole difference
 * between a link and a [proposal](crate::suggest).
 *
 * A row this is `None` on is a suggestion knobas made and nobody has
 * accepted. It is not in the graph: [`entries_of`] reads
 * `knobas.confirmed_link`, which cannot contain it, and the tray reads
 * `knobas.proposed_link`, which cannot contain anything else. Every link
 * that predates migration `0007` carries its own `created_at` here.
 *
 * RFC 3339 on the wire. A confirmed row is in `EntityDetail.links` and
 * never in `roomSuggestions`; an unconfirmed one the other way round, and
 * the two reads cannot overlap. Every link drawn by hand is confirmed the
 * moment it is written.
 */
confirmed_at: string | null, 
/**
 * The named detection rule that proposed this link, or `None` for one a
 * person drew.
 *
 * Deliberately not a closed vocabulary: rules are expected to grow, and a
 * constrained column would make each new one a migration. The *class*
 * below is the closed axis.
 *
 * Examples: `"branch_name_key"`, `"similar_text"`. Branch on `rule_class`,
 * and show `rule` only as provenance: a closed list in the shell would
 * have to ship before a new detector could.
 */
rule: string | null, 
/**
 * Which class of evidence [`rule`](Self::rule) is, or `None` for a link a
 * person drew.
 *
 * The axis a reader calibrates trust on and the one a badge should show:
 * `exact_key` is "the two ends name each other", `similarity` is a guess
 * from overlapping text, `source_relation` is a relation the source system
 * already states.
 */
rule_class: "exact_key" | "similarity" | "source_relation" | null, 
/**
 * Why knobas proposed this link, in the detector's own words -- "the
 * branch name contains PAY-231".
 *
 * Stored, not rendered from [`rule`](Self::rule) at display time: a
 * suggestion whose reason cannot be shown is not shippable (#41), and a
 * reason assembled by whichever surface happens to draw it is one that
 * can be missing from the next surface. `link_proposal_chk` (migration
 * `0007`) refuses an unconfirmed row without one.
 *
 * `None` for a link a person drew. Raw text: render it as text.
 */
reason: string | null, };

/**
 * One set, as its editor draws it (R-7.7): the row, and the tickets it shows
 * on now (*Shows on N tickets now*).
 */
export type LinkSetDetail = { 
/**
 * The tickets it shows on now, in key order.
 */
shows_on: Array<RuleTicket>, 
/**
 * A slug: `#/sets/<id>` opens the set.
 */
id: string, name: string, description: string, members: Array<LinkSetMemberRow>, 
/**
 * Its patterns, in order (#868).
 */
patterns: Array<LinkSetPatternRow>, rules: Array<LinkSetRuleRow>, 
/**
 * How many live tickets its rules match now.
 */
tickets: number, 
/**
 * How many of those are yours.
 */
mine: number, };

/**
 * What `save_link_set` takes: the whole set, members and rules in order.
 */
export type LinkSetDraft = { 
/**
 * `None` makes a new set with an id from its name; an id saves over that
 * set.
 */
id: string | null, name: string, description: string, members: Array<LinkSetMember>, 
/**
 * The patterns, in order, after the fixed members. Absent reads as
 * none, so a draft written before #868 still saves.
 */
patterns: Array<LinkSetPattern>, rules: Array<LinkSetRule>, };

/**
 * R-7.10's hint (#870): "You linked Dashboard_Build by hand to PAY-240 and 2
 * more tickets under PAY-200. Add it to Payout stack?"
 *
 * An item linked by hand to two or more tickets under the epic a set's rule
 * names, which the set does not hold yet.
 */
export type LinkSetHint = { set_id: string, set_name: string, 
/**
 * The epic the set's rule names, a ticket entity id.
 */
epic: string, 
/**
 * What *Add to …* adds: the item, as a member of its kind.
 */
member: LinkSetMember, 
/**
 * The item's title.
 */
title: string, 
/**
 * The tickets under the epic it is linked to by hand, in key order.
 */
tickets: Array<string>, };

/**
 * One fixed member: an entity of one of [`MEMBER_KINDS`].
 */
export type LinkSetMember = { kind: LinkSetMemberKind, entity_id: string, };

/**
 * [`MEMBER_KINDS`] as the generated TypeScript's `LinkSetMemberKind` union
 * (#1326): what a link-set member can be, R-7.7's five panels. Never built:
 * the wire carries the kind as a string, checked against the list, and this
 * enum only names the union a member's `kind` is in the shell.
 * `the_kind_unions_are_the_kind_lists` pins it to the list.
 */
export type LinkSetMemberKind = "repo" | "pipeline" | "page" | "asset" | "monitor";

/**
 * A member as the views draw it: a fixed member, or one item a pattern
 * matches.
 */
export type LinkSetMemberRow = { kind: LinkSetMemberKind, entity_id: string, 
/**
 * The item's title, or `None` when knobas has no entity of that id.
 */
title: string | null, 
/**
 * The line beside the name (#1008): for an asset its type and
 * environment; for a mirrored item its declared chain of groups (a
 * pipeline's project path), else its declared project and parent chain
 * (a page's space and path). `None` where nothing is declared.
 */
line: string | null, 
/**
 * The dot (#1008): a pipeline's newest run's declared state, a mirrored
 * item's own declared state, an asset's health. `None` draws no tone.
 */
state: StateChip | null, };

/**
 * One pattern member (R-7.2): a kind of [`PATTERN_KINDS`] and a query in the
 * search grammar that reads as a [`knobas_search::Pattern`].
 *
 * Issue #868. For pipelines and pages; the query reads like `project:Payouts
 * name:Payout_*`.
 */
export type LinkSetPattern = { kind: LinkSetMemberKind, query: string, };

/**
 * What the pattern dialog shows (R-7.7, "Matches now: N"): what the query
 * would match, the projects to pick from, and why the query is not a
 * pattern, if it is not.
 */
export type LinkSetPatternPreview = { count: number, matches: Array<LinkSetMemberRow>, projects: Array<LinkSetPatternProject>, 
/**
 * `None` for a query that is a pattern; then *Add pattern* saves it.
 */
refusal: string | null, };

/**
 * A project or space the items of a pattern's kind sit in, for the
 * pattern dialog's dropdown.
 */
export type LinkSetPatternProject = { source_id: string, key: string, name: string | null, 
/**
 * The project's chain of names, outermost first and ending in its own
 * (#1019: `Payments`, `Payouts`); empty for a source whose projects do
 * not nest.
 */
path: Array<string>, };

/**
 * A pattern member as the editor draws it: its query and what it matches
 * now, "(N match now)".
 */
export type LinkSetPatternRow = { kind: LinkSetMemberKind, query: string, 
/**
 * How many live items it matches now.
 */
count: number, 
/**
 * The first [`MATCHES_SHOWN`] of them, by title.
 */
matches: Array<LinkSetMemberRow>, };

/**
 * One set, as the Link sets view lists it (R-7.6).
 */
export type LinkSetRow = { 
/**
 * A slug: `#/sets/<id>` opens the set.
 */
id: string, name: string, description: string, members: Array<LinkSetMemberRow>, 
/**
 * Its patterns, in order (#868).
 */
patterns: Array<LinkSetPatternRow>, rules: Array<LinkSetRuleRow>, 
/**
 * How many live tickets its rules match now.
 */
tickets: number, 
/**
 * How many of those are yours.
 */
mine: number, };

/**
 * One rule (R-7.1): a ticket entity id for `epic`, `parent` and `manual`;
 * `<source id>:<project key>` for `project`.
 */
export type LinkSetRule = { kind: LinkSetRuleKind, value: string, };

/**
 * [`RULE_KINDS`] as the generated TypeScript's `LinkSetRuleKind` union
 * (#1326): what a rule matches on, R-7.1. Never built, as [`MemberKind`]
 * is not, and pinned to the list by the same test.
 */
export type LinkSetRuleKind = "epic" | "parent" | "project" | "manual";

/**
 * A rule as the views draw it.
 */
export type LinkSetRuleRow = { kind: LinkSetRuleKind, value: string, 
/**
 * The rule's ticket's title, or its project's name; `None` when knobas
 * has none.
 */
title: string | null, };

/**
 * One line of a log.
 */
export type LogLine = { 
/**
 * Its place in the whole log, from 1.
 */
number: number, 
/**
 * The time the source stamped it with, as the source wrote it (`10:06:12`).
 */
time: string | null, 
/**
 * The line, without the source's stamp and status marks.
 */
text: string, 
/**
 * `err` or `warn` where the source marked the line so; `None` otherwise.
 */
tone: Tone | null, 
/**
 * The index into [`RunLog::steps`] of the step the line belongs to;
 * `None` for a line before, between or after the steps.
 */
step: number | null, };

/**
 * One step of a run, as a range of its log's lines.
 */
export type LogStep = { 
/**
 * The step's name as the source gave it.
 */
name: string, 
/**
 * The step's first line (its heading) and its last, both in the whole log.
 */
first: number, last: number, 
/**
 * How long it took, in seconds, where the log's stamps say.
 */
seconds: number | null, 
/**
 * `err` when a line in it is an error, else `warn` when one is a warning.
 */
tone: Tone | null, };

/**
 * One room's tickets as the mini board draws them.
 *
 * Issue #177.
 */
export type MiniBoard = { 
/**
 * The observed status columns, in display order: the four of [`LEADING`]
 * first where the corpus shows them, every other observed status after
 * them alphabetically, and the terminal group last.
 */
columns: Array<MiniBoardColumn>, 
/**
 * Per source that put a card on this board, the statuses its own corpus
 * shows -- what the ticket detail's status select falls back to when the
 * workflow read fails (#179, and #498 for why it is the fallback rather
 * than the offer).
 */
sources: Array<SourceStatuses>, 
/**
 * The board in lanes (#939), when it was asked for in lanes
 * ([`Scope::lanes`]); empty otherwise. Biggest first. Each lane names
 * its cards by id; the cards themselves are in [`Self::columns`], once.
 */
lanes: Array<MiniBoardLane>, 
/**
 * How many lanes there are past [`Self::lanes`], which a read that
 * asked for its first lanes ([`Lanes::first`]) did not send.
 */
more_lanes: number, };

/**
 * One ticket on the board.
 *
 * Issue #177.
 */
export type MiniBoardCard = { 
/**
 * The ticket's stable in-app address, which is what a click opens.
 */
entity_id: string, 
/**
 * Which source this ticket came from -- the same source whose statuses
 * [`MiniBoard::sources`] lists.
 */
source_id: string, 
/**
 * The source's own key for it (`PAY-231`), i.e. `entity_id` past its
 * namespace.
 */
key: string, title: string, 
/**
 * The ticket's own status, in the source's words (#1423): the status
 * it stands in on [`read_scoped`]'s board, carried on the card because
 * a saved view's column gathers several statuses under the view's name
 * ([`MiniBoardColumn::status`]). `None` for the terminal group.
 */
status: string | null, 
/**
 * `None` where the record carries no readable priority; see
 * [`priority_read!`].
 *
 * The card omits it rather than guessing (ADR-0007).
 */
priority: string | null, 
/**
 * The ticket's type in its source's words (`Story`, `Bug`), at the
 * declared `issue_type` path (#810); `None` on a miss, drawn as nothing.
 */
issue_type: string | null, 
/**
 * The account it is assigned to, at the declared `assignee` path, in the
 * spelling the source uses for an account; `None` for an unassigned
 * ticket and on a miss.
 */
assignee: string | null, 
/**
 * When the source last changed it, or `None` where it never said.
 *
 * RFC 3339 on the wire.
 */
updated_at: string | null, 
/**
 * The ticket's declared parent chain as one line (its epic, for Jira):
 * `knobas_core::path_line_read!`, the line every other row draws.
 *
 * `None` where no parent is declared or it misses.
 */
path: string | null, 
/**
 * The title of the item its direct parent names (#894): the epic's
 * name beside [`Self::path`]'s key, `PAY-200 Payout reliability`. The
 * parent's last declared position, its first name; the mirrored live
 * item of the parent's kind at `<source>:<name>` in the same source,
 * and only if that item answers to the name as its declarations say.
 * `None` for a ticket with no parent, an epic the mirror does not hold,
 * and an item that does not answer to the name (ADR-0007).
 */
parent_title: string | null, 
/**
 * Whether [`Self::assignee`] is one of the configured usernames
 * ([`Scope::identity`]), ignoring case. `false` when no identity was
 * given ([`read`], [`read_capped`]); the `mini_board` command always
 * gives it, and the rooms' tile simply does not draw it.
 */
mine: boolean, 
/**
 * The labels it carries, at the declared `labels` path (#811's field),
 * in the record's order; empty on a miss. The Tickets view's *Label*
 * lanes read the first (#813, R-3.5).
 */
labels: Array<string>, 
/**
 * How many comments it has, at the declared `comment_count` path
 * (#894): a whole JSON number, `Some(0)` when the source said none, and
 * `None` on a miss. The card draws "N comments" above zero only.
 */
comment_count: number | null, 
/**
 * The signals around it (#815, R-3.8), in the order a card draws them:
 * [`crate::signals`]' reader, the one `has:` asks too.
 */
signals: Array<Signal>, };

/**
 * One column: a status, and the cards standing in it (#177).
 *
 * The count the column header shows is [`total`](Self::total) (#753): on a
 * capped read `cards` holds only the newest, so its length is not the count.
 */
export type MiniBoardColumn = { 
/**
 * The status, in the source's own words -- or `None` for the terminal
 * group: tickets whose mirrored record carries no status this read
 * recognizes. The words on screen for that group are the shell's ("No
 * status"); what crosses the bridge is the absence itself.
 *
 * On a saved view's board (a Jira view's, #877) it is the view
 * column's **name**, which gathers several statuses: read a ticket's
 * status from [`MiniBoardCard::status`], never from here (#1423).
 */
status: string | null, 
/**
 * Newest first: all of them, or the newest of them where the read was
 * capped ([`read_capped`]).
 */
cards: Array<MiniBoardCard>, 
/**
 * How many tickets stand in this column, `cards` or not (#753). Equal to
 * `cards.len()` on an uncapped read; the header draws this one.
 */
total: number, };

/**
 * One lane of a laned board: the value its tickets share, how many they
 * are, and its own columns, each capped and counted as a board's column is.
 *
 * Issue #939.
 */
export type MiniBoardLane = { 
/**
 * What its tickets share, in the source's words; `""` for the lane of
 * tickets with none of it.
 */
key: string, 
/**
 * How many tickets stand in it.
 */
count: number, 
/**
 * Its statuses, in the board's column order; only those it has.
 */
columns: Array<LaneColumn>, };

/**
 * What *New page* asks for (R-10.16).
 *
 * Issue #862.
 */
export type NewPage = { 
/**
 * The page it goes under, a mirrored page of the space.
 */
parent: string, title: string, 
/**
 * The editor's plain text: `## ` a section, `- ` a list item.
 */
text: string, };

/**
 * Everything the note view draws for one note (#46).
 *
 * Deliberately **not** [`EntityDetail`], and the difference is not tidiness:
 * that DTO is shaped around a mirrored item -- a source, a `payload`, a
 * `web_url`, a `synced_at` -- and a note has none of those. Half of it would
 * be filled in with placeholders that a reader could not tell from real ones.
 * A note is an entity and shares the address space; it is not a mirror row.
 *
 * `refs` and `links` overlap and both are here because they answer different
 * questions. `refs` is *the body's own list*, in body order, including the
 * ones that resolve to nothing -- which is what the editor draws chips from
 * and what makes an unresolved ref visible (story 10). `links` is the panel
 * #53 built: every link this note takes part in, in either direction, so the
 * note shows what points at it as well as what it points at.
 */
export type NoteDetail = { note: NoteRow, 
/**
 * The `[[refs]]` the body names, in body order, resolved where they
 * resolve.
 */
refs: Array<NoteRef>, 
/**
 * Every link this note takes part in, each with its other end resolved --
 * the ref links it derived, plus anything drawn by hand from either side.
 */
links: Array<LinkEntry>, };

/**
 * One link a caller asks a new note to be born with (#502).
 *
 * An **input** DTO: the frontend sends these and nothing sends them back. It
 * is a pair rather than two named fields (`capturedIn`, `capturedFrom`)
 * because the command's job is *draw these links with the note*, and the two
 * words are the caller's vocabulary, not the command's -- exactly as
 * [`create_link_inner`] takes a relation rather than knowing what `blocks`
 * means. The capture window (#503) and the in-app *New note* pass the same
 * two, and a third caller with a third relation needs no argument here.
 *
 * The note is always the **from** end, so `captured-in` reads *captured in*
 * from the note and *captured here* from the context; see
 * [`knobas_core::note::BornLink`].
 */
export type NoteLinkInput = { 
/**
 * The other end's entity id.
 */
target_id: string, 
/**
 * The relation the link carries, folded to lower case like every other
 * relation this module writes.
 *
 * Required, and blank is refused rather than defaulted. `create_link`'s
 * relation is an `Option` and its absence means *nobody named one*, which
 * [`DEFAULT_RELATION`] answers; here the caller is drawing a link the
 * reader never saw a dialog for, so `""` is a caller bug and a link
 * labelled `related` that nobody asked for would be the wrong sentence in
 * the panel rather than an error anybody could find.
 */
relation: string, };

/**
 * One `[[ref]]` the body names, and what it points at -- if anything.
 *
 * `target` is `None` for an **unresolved** ref: nothing carries that id, so it
 * creates no link and is drawn as unresolved rather than as plain text, which
 * is how a typo is discoverable. A `target` whose `deleted_at` is set is the
 * other case entirely: the ref resolves, the link exists, and the chip is
 * marked withdrawn.
 */
export type NoteRef = { 
/**
 * The text between the brackets as the body wrote it, trimmed of the
 * whitespace around it ([`parse_refs`]): `[[ jira:PAY-231 ]]` is
 * `jira:PAY-231`.
 *
 * Not necessarily a well-formed entity id: a typo is a ref that resolves
 * to nothing, and showing it back is how the user finds the typo (#46
 * story 10).
 */
target_id: string, 
/**
 * The entity it names, when one exists.
 *
 * [`None`] is an **unresolved** ref: it is visible as unresolved and it
 * creates no link. `Some` with `deleted_at` set is a ref whose target was
 * withdrawn upstream -- still a ref, still a link, and marked (story 9).
 */
target: LinkEnd | null, };

/**
 * One note.
 *
 * Notes are the first kind knobas *owns* rather than mirrors: nothing synced
 * this and nothing can re-fetch it, which is why it is in the backup and never
 * written to a source.
 */
export type NoteRow = { 
/**
 * `note:<uuid>`, and also the id of this note's `knobas.entity` row.
 *
 * Also this note's entity id, so it links like anything else.
 */
id: string, 
/**
 * The note's name. Never blank -- see [`UNTITLED`].
 *
 * A note the user has not named is "Untitled note".
 */
title: string, 
/**
 * Markdown the user typed. **Untrusted text**: render it as text, never as
 * markup (roadmap §4 gotcha 7). It is the user's own here rather than a
 * source's, which changes who is at fault and nothing else.
 *
 * There is no `{@html}` anywhere in the shell: a `[[ref]]` becomes a chip
 * by parsing the body into tokens and drawing them, not by turning the
 * string into HTML.
 */
body_md: string, 
/**
 * RFC 3339 on the wire.
 */
created_at: string, 
/**
 * RFC 3339 on the wire. Moves on every save.
 */
updated_at: string, };

/**
 * What an export wrote.
 *
 * *Export notes as Markdown* (#832).
 */
export type NotesExport = { 
/**
 * How many notes were written, one file each.
 */
count: number, 
/**
 * The folder they are in, absolute.
 */
folder: string, };

/**
 * The `notification:clicked` payload (#339): the `address` the notification
 * was sent with.
 */
export type NotificationClicked = { address: string, };

/**
 * What the frontend asks to have shown: the `notify` command's argument
 * (#339).
 *
 * `address` is where the click goes, the inbox's own `addressOf` hash
 * (`lib/inbox/address.ts`, shared with the inbox row's *Open* since #447);
 * the backend carries it unread and hands it back on
 * `EVENTS.notificationClicked`, so both ends of the click path stay the
 * frontend's.
 */
export type NotificationDraft = { title: string, body: string, 
/**
 * Where the click goes -- the store's own `addressOf`, carried through
 * unread so the two ends of the click path stay the frontend's.
 */
address: string, };

/**
 * Which panel a [`DocBlock::Panel`] is.
 */
export type PanelKind = "info" | "note" | "warning" | "tip";

/**
 * The field a parameter is asked with. Tagged on `type`.
 *
 * - `text`: Free text: anything typed is the value.
 * - `select`: One of a fixed list, in the source's order.
 * - `checkbox`: On or off, each with the value it sends.
 */
export type ParameterKind = { "type": "text" } | { "type": "select", options: Array<ParameterOption>, } | { "type": "checkbox", checked: string, unchecked: string, };

/**
 * One value a [`ParameterKind::Select`] offers.
 */
export type ParameterOption = { 
/**
 * What the run sends.
 */
value: string, 
/**
 * What the source shows for it, where it differs from the value.
 */
label: string | null, };

/**
 * How one parameter is asked for when a run is started (#852): the
 * source's own spec of it, parsed. A parameter the source says nothing
 * about, or says something the adapter cannot read, is [`ParameterKind::Text`].
 */
export type ParameterSpec = { 
/**
 * The parameter's full name, as in [`PipelineSettings::parameters`].
 */
name: string, 
/**
 * What the source shows instead of the name, where it sets one.
 */
label: string | null, 
/**
 * What the source says about the value, such as what empty means.
 */
hint: string | null, kind: ParameterKind, };

/**
 * Why a pipeline is on a job (#850, R-9.1, R-9.2). Tagged on `kind`, so a
 * link set (#866) joins as one more arm without changing these.
 *
 * - `branch`: One of its runs built a branch the job reaches: a reached
 *   branch, by its title, or the branch a reached pull request is opened from
 *   (its declared `head`). *your branch*.
 * - `pr_check`: One of its runs checked a pull request the job reaches: it ran
 *   on the pull request's declared `commit` (its head), or the job reaches the
 *   run through the pull request. *PR check*.
 * - `linked`: The job reaches the pipeline, or one of its runs, some other
 *   way: a direct link, the parent's links (R-7.3), or a link set (#866).
 *   `step` and `origin` are the reach's own ([`knobas_core::reach::Reached`]).
 */
export type PipelineReason = { "kind": "branch", branch: string, } | { "kind": "pr_check", pr: string, } | { "kind": "linked", step: string, origin: ReachOrigin, };

/**
 * One repository a pipeline builds.
 *
 * Issue #851.
 */
export type PipelineRepository = { 
/**
 * What the source calls the checkout.
 */
name: string, 
/**
 * Where it fetches from, never with a credential. `None` where the
 * adapter could not take the credential out, or the source names none.
 */
url: string | null, 
/**
 * The branch a run builds when it names none.
 */
default_branch: string | null, 
/**
 * The mirrored repository this URL is, filled by the app (the adapter
 * cannot see another source's mirror) and `None` where the mirror holds
 * none. An adapter leaves it `None`.
 */
entity_id: string | null, };

/**
 * One pipeline in the Builds list (R-9.4a, #848).
 *
 * `source_name` is the configured source's display name: the one place the
 * source's word appears, in the list's header line (ADR-0022). `recent` is
 * its last runs newest first, at most [`PIPELINE_RECENT`]: the strip, the
 * latest time, and whether the latest run failed.
 */
export type PipelineRow = { entity_id: string, source_id: string, source_name: string, name: string, group: string | null, 
/**
 * The chain of groups it sits in, outermost first (#849): the declared
 * `group_path`. Empty where the source declares none or the record
 * carries none; the list then draws it under its `group`.
 */
path: Array<string>, 
/**
 * Whether the reader pinned it (#849): knobas-local, never the
 * source's ([`PINS_SETTING`]).
 */
pinned: boolean, web_url: string | null, recent: Array<RunRow>, };

/**
 * A pipeline's settings, read live (#851).
 *
 * A secret parameter's value is never sent.
 */
export type PipelineSettings = { 
/**
 * The repositories the pipeline checks out, in the source's order. Most
 * pipelines build one.
 */
repositories: Array<PipelineRepository>, 
/**
 * What starts a run on its own, in the source's order.
 */
triggers: Array<PipelineTrigger>, 
/**
 * The parameters every run starts from, in the source's order. A secret's
 * value is never sent (`Setting::secret`).
 */
parameters: Array<Setting>, 
/**
 * The write op a parameter's default is changed through, where the
 * source declares one (`set_parameter`); `None` draws them read-only.
 */
parameter_write_op: string | null, 
/**
 * How each parameter is asked for in the Run… dialog (#852, R-9.7): one
 * per entry of `parameters`, by name, in the same order. Absent from an
 * older peer's answer, which reads as every parameter plain text.
 */
parameter_specs: Array<ParameterSpec>, 
/**
 * The chain the pipeline is a step of (#857, R-9.13): every pipeline it
 * is joined to by the source's dependencies (TeamCity's snapshot
 * dependencies), this one included, in the order they run. Empty for a
 * pipeline in no chain, and absent from an older peer's answer.
 */
chain: Array<ChainStep>, 
/**
 * The steps a run of the pipeline takes (#1016, R-9.10): the enabled
 * ones by name, in the order a run's log numbers them. The run page draws
 * the ones a running run's log has not reached yet as *not started*.
 * Empty where the source does not name every step ahead of time, and
 * absent from an older peer's answer.
 */
steps: Array<string>, };

/**
 * One trigger of a pipeline: its kind in the source's word and what it
 * fires on.
 *
 * Issue #851.
 */
export type PipelineTrigger = { 
/**
 * The kind of trigger, in the source's word (`VCS`, `Schedule`).
 */
name: string, 
/**
 * When and for what it fires, as labelled values (`When`: `daily at
 * 02:00`). Only what the adapter knows to be safe to show.
 */
facts: Array<Fact>, 
/**
 * Switched off at the source.
 */
disabled: boolean, };

/**
 * One project the live corpus shows.
 *
 * Flat, and carrying its source rather than being grouped under it: the
 * switcher wants a room list, and grouping here would only be ungrouped
 * there.
 *
 * ADR-0010, issue #208.
 */
export type Project = { 
/**
 * Which source shows it.
 *
 * Half of the identity, not decoration: a project key is unique only
 * inside its own source, so this is what keeps two sources' `PAY` two
 * projects and two rooms.
 */
source_id: string, 
/**
 * The source's own key for it (`PAY`).
 */
key: string, 
/**
 * The source's own name for it, or `None` where no name is readable.
 *
 * `None` rather than a copy of the key: what to *label* a room with is
 * the shell's decision, and a name knobas invented would be
 * indistinguishable on the wire from one the source said.
 */
name: string | null, };

/**
 * Everything said for and against one proposal.
 *
 * Issue #836.
 */
export type ProposalEvidence = { link_id: string, 
/**
 * The reason the rule stored, unchanged.
 */
reason: string | null, 
/**
 * The text the evidence was found in, with the match marked. Empty for a
 * rule whose evidence is not in any end's text (a relation the source
 * records).
 */
excerpts: Array<EvidenceExcerpt>, 
/**
 * *Also found*: what supports the proposal. May be empty.
 */
support: Array<string>, 
/**
 * *Speaks against it*. Never empty: [`NOTHING_AGAINST`] when nothing does.
 */
against: Array<string>, };

/**
 * A date's protocol: the note, and what became of publishing it.
 *
 * `CONTEXT.md`'s **standup protocol**: a note, one per date, holding
 * attendees, per-person notes and action items. Not a kind of its own, which
 * is why this carries a `note_id` and everything else about it is read through
 * `get_note` like any other note.
 */
export type Protocol = { 
/**
 * The date this protocol is for, as the reader's own calendar spells it.
 *
 * `YYYY-MM-DD` on the wire.
 */
day: string, 
/**
 * The note's entity id.
 */
note_id: string, 
/**
 * The title the published page carries -- what a second publish for this
 * date would collide with, shown so the reader knows what they are about
 * to put in the wiki.
 */
page_title: string, 
/**
 * The publication, when there has been one.
 */
publication: Publication | null, 
/**
 * The tickets this protocol's action items filed that it does not link
 * yet, oldest first (#1951): each create that still owes the note its
 * link, sent or not, and not withdrawn. A reload reads an item filed
 * here as on its way from this (pending, sent, or held as unconfirmed:
 * [`ActionItemTicket::may_have_landed`](crate::commands::entity::ActionItemTicket::may_have_landed),
 * #1966) rather than offering *Create ticket* again; the read links
 * each one whose ticket the mirror now holds, and names it no more.
 */
owed_tickets: Array<ActionItemTicket>, };

/**
 * What became of a date's publication, as far as knobas can tell.
 */
export type Publication = { 
/**
 * The write queue row that carries it.
 */
write_id: number, 
/**
 * The queue's own word for where the write stands.
 */
state: WriteState, 
/**
 * The source's sentence, when it had one. Untrusted source text.
 */
detail: string | null, 
/**
 * The page, as an entity id, once the source has named it.
 *
 * `None` while the write is still in flight -- and *not* the same fact as
 * [`linked`](Self::linked) being false, which additionally means the
 * mirror has not caught up.
 */
page_entity_id: string | null, 
/**
 * Whether note and page are linked yet.
 */
linked: boolean, };

/**
 * Where protocols are published: which Confluence, and under which page.
 *
 * Both halves, because neither is enough. The source id is what routes the
 * write (interfaces §4.1 makes it the entity namespace), and with two
 * Confluence sources configured a parent id alone would name a page in an
 * instance nobody chose -- story 68 in one struct field.
 *
 * `parent` is a **mirrored page entity id** (`confluence:98400`), not a raw
 * content id and not a space: `WriteOp::CreatePage` says so, and it is what a
 * reader picks out of the mirror. The space key the op also needs is *not*
 * stored here -- it is read off the parent's own record at publish time, so
 * the pair can never disagree about which space the parent is in.
 *
 * Issue #289, spec #272 stories 67-68.
 */
export type PublishTarget = { 
/**
 * The Confluence source's instance id.
 */
source_id: string, 
/**
 * The parent page, as an entity id.
 */
parent: string, };

/**
 * Where a reached entity's path came from.
 *
 * Tagged on `kind`; #866's link sets joined as the third arm without
 * changing the two before it.
 *
 * - `own`: The ticket's own links, its context's adds, and where those lead.
 * - `parent`: The parent ticket's own links (R-7.3), labelled "Parent
 *   PAY-231".
 * - `link_set`: A member of a link set whose rule matches the ticket (#866,
 *   ADR-0025), labelled with the set's name.
 *
 * A link set's arm is named as the reader named the set.
 */
export type ReachOrigin = { "kind": "own" } | { "kind": "parent", 
/**
 * The parent ticket's entity id.
 */
ticket: string, } | { "kind": "link_set", 
/**
 * The set's id, its address: `#/sets/<set>`.
 */
set: string, 
/**
 * The set's name, as the reader gave it.
 */
name: string, };

/**
 * One entity a ticket reaches.
 *
 * Issue #817.
 */
export type Reached = { entity_id: string, 
/**
 * What `entity_id` is: `pr`, `build`, `asset`, `monitor`.
 */
kind: string, 
/**
 * The first step of the path that brought it in: an entity id. Equal to
 * `entity_id` when the entity is itself the first step -- linked to the
 * ticket directly, or one of the parent's own links.
 *
 * What the row's reason names.
 */
step: string, origin: ReachOrigin, };

/**
 * A write the source's credential cannot make, or one field of it the write
 * cannot carry, and why, in words a person can act on: *This TeamCity user cannot run builds in Payout: it lacks the
 * Run build permission*.
 */
export type RefusedWrite = { entity: string, op: string, 
/**
 * The [`WriteAsk::field`] refused, or `None` for the write itself.
 */
field?: string, reason: string, };

/**
 * A ticket a rule or a set applies to.
 */
export type RuleTicket = { entity_id: string, title: string, mine: boolean, };

/**
 * A range of a run's log (#854).
 */
export type RunLog = { 
/**
 * The lines asked for, in order.
 */
lines: Array<LogLine>, 
/**
 * The run's steps over the **whole** log, in order, whether or not their
 * lines are in [`lines`](Self::lines).
 */
steps: Array<LogStep>, 
/**
 * How many lines the whole log has.
 */
total: number, };

/**
 * One preset, as `save_run_preset` takes it and `run_presets` answers it.
 *
 * Issue #852: what a run of one pipeline starts from, saved under a name.
 * Knobas-local.
 */
export type RunPreset = { 
/**
 * The pipeline's entity id; never checked against the mirror.
 */
pipeline_id: string, 
/**
 * As the reader typed it, trimmed.
 */
name: string, 
/**
 * `None` is the pipeline's default branch.
 */
branch: string | null, 
/**
 * The overrides only, in the order the reader saw them.
 */
parameters: Array<BuildParameter>, clean_checkout: boolean, first_in_queue: boolean, };

/**
 * One run of a pipeline, as the Builds view draws it (#848): every field
 * read through the run kind's declared paths (ADR-0007), and nothing else.
 *
 * `started` and `finished` are the declared ISO 8601 instants, basic or
 * extended (`crate::payload::parse_instant`); a run in flight has no
 * `finished`, a queued one neither. `duration_seconds` is the one from the
 * other, only where both are read.
 *
 * On the wire `started` and `finished` are RFC 3339, `None` while queued or in
 * flight.
 */
export type RunRow = { entity_id: string, 
/**
 * The run's number in the source's own spelling; a queued TeamCity
 * build has none yet.
 */
number: string | null, 
/**
 * The declared state chip (#666): its text in the source's word and the
 * tone the shell draws it in.
 */
state: StateChip | null, branch: string | null, started: string | null, finished: string | null, 
/**
 * Whole seconds, only where both instants were read.
 */
duration_seconds: number | null, 
/**
 * The source's own timestamp for the row, what runs are ordered by.
 */
updated_at: string | null, web_url: string | null, 
/**
 * How the run was started (#961): the declared `trigger`, in the
 * shell's words.
 *
 * `None` where the declaration misses.
 */
trigger: Trigger | null, 
/**
 * Who started it, as the source spells the account: the declared
 * `started_by`. A run a change or a schedule started has none.
 */
started_by: string | null, 
/**
 * Whether `started_by` is one of the configured usernames, the identity
 * behind `@me` (*Started by me*, *you*).
 */
started_by_me: boolean, 
/**
 * The commit it built: the first of the declared `revision`s (#710), a
 * full sha.
 */
revision: string | null, 
/**
 * The mirrored commit `revision` names (#1004): the live item of kind
 * `commit` whose declared `commit` is that sha, what the run page's
 * *Commit* opens. `None` where the mirror holds no such commit.
 */
commit_id: string | null, 
/**
 * The machine it ran on: the declared `agent` (#1004). A queued run has
 * none.
 */
agent: string | null, 
/**
 * What it is doing now, in the source's words: the declared
 * `current_step` (#1004). Only a run in flight has one.
 */
current_step: string | null, 
/**
 * Its test summary (#1448, R-9.6): the declared `tests_failed`,
 * `tests_passed` and `tests_ignored`. `None` for a run that ran no
 * tests, which declares none of the three.
 */
tests: TestSummary | null, };

/**
 * One test of a run.
 */
export type RunTest = { 
/**
 * The source's id for the test across runs: what [`TestOutcome`]s are
 * asked by. `None` where the source has none.
 */
id: string | null, 
/**
 * The test's full name as the source gave it.
 */
name: string, 
/**
 * The module or class it is in: its name up to the last `::` or `.`.
 */
module: string | null, status: TestStatus, 
/**
 * The source mutes its failure.
 */
muted: boolean, 
/**
 * It passed in the run before and fails in this one.
 */
new_failure: boolean, duration_ms: number | null, 
/**
 * What the source says about a failure: the assertion, the stack.
 */
details: string | null, };

/**
 * A run's tests (#855).
 */
export type RunTests = { 
/**
 * The run's tests, failed ones first, at most [`TESTS_MAX`].
 */
tests: Array<RunTest>, 
/**
 * The run has more tests than [`tests`](Self::tests) holds.
 */
truncated: boolean, };

/**
 * Why a section is edited in the source and not in knobas (R-10.13): a text
 * box round-trips plain text, and what it cannot hold would be lost.
 *
 * - `table`: A table.
 * - `code`: A code block.
 * - `macro`: A macro, a layout, a mention, a page or ticket link: markup of
 *   the source's own that text cannot carry. An info, note, warning or tip
 *   panel is edited (#975) unless it carries more than a title.
 * - `formatting`: Bold, italics, links, a nested or a numbered list, which the
 *   text box would flatten.
 */
export type SectionRefusal = "table" | "code" | "macro" | "formatting";

/**
 * One named value of a [`Block::Settings`].
 */
export type Setting = { name: string, 
/**
 * The value as the source holds it. Empty for a `secret` one: the
 * adapter never sends a password's value to the shell.
 */
value: string, 
/**
 * Set by something above this entity (a TeamCity project's parameter
 * seen on a configuration).
 */
inherited: boolean, secret: boolean, 
/**
 * Whether the block's `write_op` may change it.
 */
editable: boolean, description: string | null, };

/**
 * One signal on one ticket: what the card and the list row draw.
 *
 * Issue #815.
 */
export type Signal = { kind: SignalKind, 
/**
 * The words drawn beside the dot (*blocked by OPS-77*).
 *
 * Other examples: *build failed*, *PR #142 open*.
 */
text: string, tone: Tone, };

/**
 * Which fact a [`Signal`] states: the four values `has:` takes.
 *
 * - `failing-build`: A build in the ticket's reach is red.
 * - `blocker`: Something is in the ticket's way.
 * - `open-pr`: A pull request of the ticket's is open.
 * - `review-for-me`: One of those asks the user for a review.
 *
 * Issue #815.
 */
export type SignalKind = "failing-build" | "blocker" | "open-pr" | "review-for-me";

/**
 * Which source an entity came from, as the detail view names it.
 *
 * Every field falls back to the source id, because `run_once` syncs sources
 * that have no `knobas.source_config` row (tests, ad-hoc imports, interfaces
 * §1). An entity whose source was deleted still has to render.
 */
export type SourceRef = { id: string, 
/**
 * The configured display name, falling back to the id: `run_once` syncs
 * sources that were never configured, so there is not always a name.
 */
display_name: string, 
/**
 * The *adapter* kind (`jira`, `mock`), not the instance id.
 */
adapter_kind: string, 
/**
 * Whether the user has this source turned on, **as of this read** --
 * derived in the [`DETAIL`] statement, never stored (issue #204).
 *
 * The detail reaches past `sync.live_item` on purpose (§5a), so it opens
 * entities every other reader hides -- and since migration `0012` there
 * are two reasons a reader hides one. `deleted_at` marks the first
 * (withdrawn upstream); this marks the second (turned off by the user),
 * and the banner needs both because the remedies differ: nothing undoes
 * a withdrawal, one click undoes a disable -- which is also why it is
 * derived: a stored marker would still say "off" after that click.
 *
 * `true` for a source with no configuration row, matching `0012`'s
 * `coalesce(enabled, true)`: absence of configuration is not a decision
 * the user made.
 */
enabled: boolean, };

/**
 * The statuses one source's live ticket corpus shows, in the board's order.
 *
 * Issue #177.
 */
export type SourceStatuses = { source_id: string, 
/**
 * Never carries the terminal group: "no status" is something a ticket can
 * be *in*, not something it can be moved to.
 */
statuses: Array<string>, };

/**
 * What [`Source::view`](crate::Source::view) answers for one entity: its
 * sections, in the order the shell draws them as tabs. The first is the one a
 * detail opens on.
 */
export type SourceView = { sections: Array<ViewSection>, 
/**
 * What a reader can do to the entity, drawn as a toolbar between the
 * detail's header and the tabs, on every tab (issue #702, spec #701).
 * Empty draws no toolbar. Absent on the wire reads as empty, so a view
 * drawn before the toolbar decodes.
 *
 * A [`Block::Actions`] is for an action that belongs to one section; an
 * action is in one place or the other, never both.
 */
actions: Array<ViewAction>, };

/**
 * The four lists, and which day the first of them is about.
 *
 * Issues #288 and #932. `CONTEXT.md`'s **digest**, drawn from the mirror and
 * the activity stream for the configured usernames. Mine only (bar
 * [`infrastructure`](Self::infrastructure)): it describes the person the
 * sources were configured as, never a colleague.
 */
export type StandupDigest = { 
/**
 * The day [`yesterday`](Self::yesterday) is really about, in the reader's
 * own reckoning -- Friday, on a Monday.
 *
 * `None` when the rule found nothing inside [`LOOKBACK_DAYS`], which is a
 * week of silence and not an error. The view says so; a date filled in
 * anyway would be a heading over an empty list claiming that day was
 * quiet, when what happened is that no day was found.
 *
 * `"2026-08-28"` on the wire.
 */
yesterday_day: string | null, 
/**
 * The newest earlier day with any of the user's activity, newest first.
 */
yesterday: Array<DigestLine>, 
/**
 * What has been touched since the day's own midnight, plus the running
 * timer's target, newest first.
 */
today: Array<DigestLine>, 
/**
 * The user's items a source calls stuck or a link calls blocked.
 *
 * An item can be on this list twice, once for each reason: they are two
 * different facts about it, and collapsing them would throw one of them
 * away.
 */
blockers: Array<DigestLine>, 
/**
 * The monitor alerts since the last working day (#932, R-13.4): every
 * [alert](knobas_sync::alerts) that was open at any point from the start
 * of [`yesterday_day`](Self::yesterday_day) -- or of today, when no such
 * day was found -- to the end of today, newest first.
 *
 * **Not mine only**, unlike the three lists above: a monitor has no owner
 * in the mirror, and the standup asks what happened to the estate, not
 * what happened to you. A monitor's restart is not here: the mirror
 * records a state change, not why it changed.
 *
 * One line per alert: `kind` `"monitor"`, `verb` `"down"` or `"warn"`,
 * `reason` the duration (`was down 12 min`, `is still down`) and `at` when
 * it began.
 */
infrastructure: Array<DigestLine>, };

/**
 * What happened to one step.
 *
 * Stored as lowercase text in `knobas.start_work_step.outcome`, whose
 * `start_work_outcome_chk` (migration 0008) allows exactly these
 * spellings.
 *
 * - `pending`: Planned; nothing has been attempted.
 * - `running`: Dispatched, no answer yet -- what tells a slow step from a
 *   stuck one (story 11).
 * - `succeeded`: The effect exists at the source.
 * - `queued`: The source could not take the write, so it is a **pending
 *   write** and will go when the source can (story 15).
 *
 *   Deliberately neither a success nor a failure. The flow's completion
 *   and the write's delivery are different events and the two must not be
 *   conflated: a queued step has changed nothing at the source yet, so the
 *   sequence waits rather than reporting a step that has not happened.
 * - `failed`: The source refused, or knobas could not go on.
 * - `skipped`: The user chose not to run it (story 14).
 */
export type StartWorkOutcome = "pending" | "running" | "succeeded" | "queued" | "failed" | "skipped";

/**
 * One step of one flow.
 *
 * Issue #44. `payload` is untyped on the wire for `submit_write`'s reason:
 * `WriteOp` grows per milestone.
 */
export type StartWorkStep = { id: number, 
/**
 * The ticket the flow is about -- and the flow's identity.
 */
ticket_id: string, step: StartWorkStepKind, 
/**
 * Where the step sits in the sequence.
 *
 * The stepper renders in this order.
 */
position: number, outcome: StartWorkOutcome, 
/**
 * **The proposal**, as the user last left it: the serialized
 * `knobas_source::WriteOp` a dispatching step will submit, or the link's
 * relation.
 */
payload: unknown, 
/**
 * The `knobas.write_queue` row this step dispatched, once it has one.
 *
 * What makes a retry safe: a step whose write is still open is retried by
 * acting on *that* write rather than by queueing a second one.
 */
write_id: number | null, 
/**
 * What happened, in whoever's words. Untrusted source text.
 */
detail: string | null, updated_at: string, };

/**
 * The steps a start-work flow is made of.
 *
 * Stored as lowercase text in `knobas.start_work_step.step`, whose
 * `start_work_step_chk` (migration 0008) allows exactly these spellings --
 * and `ALL` is what the test that pins the two together walks.
 *
 * Three of the four are `knobas_source::WriteOp` identifiers and one is
 * not: [`LinkPullRequest`](Step::LinkPullRequest) is a **knobas link**,
 * created through the link store and never written to either source,
 * which is the whole reason the relationship survives regardless of what
 * Jira or Gitea happens to record (story 9).
 *
 * - `create_branch`: The branch, in the repository the user chose.
 * - `create_pull_request`: The draft pull request, from that branch.
 * - `link_pull_request`: The knobas link from the pull request back to the
 *   ticket.
 * - `transition`: The ticket, to In Progress.
 */
export type StartWorkStepKind = "create_branch" | "create_pull_request" | "link_pull_request" | "transition";

/**
 * The chip a row draws: the resolved [`KindPaths::state`].
 *
 * `EntityRow.state` on the wire. `text` is source text (or the adapter's
 * rename of it) and is drawn as text.
 */
export type StateChip = { text: string, tone: Tone, };

/**
 * One proposal, as the tray draws it: the row, and **both** its ends.
 *
 * Both, unlike [`crate::link::LinkEntry`], which resolves the end the reader is
 * not on. The tray is not read from either end -- it is read from a room -- so
 * there is no "here" for it to leave out, and #41's story 20 wants both ends
 * openable before deciding.
 */
export type SuggestionEntry = { link: LinkRow, from: LinkEnd, to: LinkEnd, };

/**
 * An item with proposals the tray does not draw, and how many (#774).
 */
export type SuggestionMore = { item: LinkEnd, 
/**
 * The item's proposals in the room that the tray did not draw.
 */
more: number, };

/**
 * One page of a room's proposals, and how many there are in all.
 *
 * `total` is the whole room, not the page -- #41's story 19 is "see how many
 * suggestions are waiting, so that I can choose when to spend attention on
 * them", and a number capped by `limit` answers a different question. Same
 * shape and same reason as [`EntityPage`].
 */
export type SuggestionPage = { rows: Array<SuggestionEntry>, 
/**
 * Every proposal in the room, before `limit`.
 *
 * The number the Proposed links badge shows.
 */
total: number, 
/**
 * Items holding more proposals than one item shows (#774), most hidden
 * first. The bound is applied by `suggest::proposals`.
 */
more: Array<SuggestionMore>, };

/**
 * One cell of a [`DocBlock::Table`].
 */
export type TableCell = { header: boolean, text: Array<DocInline>, };

/**
 * How one test came out in one run: its history, newest first (#855).
 */
export type TestOutcome = { 
/**
 * The run, as an entity id.
 */
run: string, number: string | null, branch: string | null, status: TestStatus, };

/**
 * How a test came out.
 */
export type TestStatus = "failed" | "passed" | "ignored";

/**
 * A run's test summary (#1448): how many tests failed, passed and were
 * ignored. A count the run's record leaves out while another reads is
 * zero.
 */
export type TestSummary = { failed: number, passed: number, ignored: number, };

/**
 * A ticket's thread, oldest first.
 *
 * Issue #819.
 */
export type Thread = { 
/**
 * In the source's order, oldest first.
 */
comments: Array<ThreadComment>, 
/**
 * The accounts `@me` resolves to, so the shell can mark `@name` in a body
 * by the same identity this read matched.
 */
me: Array<string>, };

/**
 * One comment of a thread.
 *
 * Issue #819.
 */
export type ThreadComment = { 
/**
 * Who wrote it, as the source names them to a person (Jira's
 * `displayName`), else their account. `None` when the record says nobody.
 */
author: string | null, 
/**
 * When it was written. `None` when the source's stamp is absent or reads
 * as no time: such a comment keeps its place in the source's order.
 *
 * RFC 3339 on the wire.
 */
at: string | null, 
/**
 * The text as the source wrote it (Jira's wiki markup), never markup here.
 */
body: string, 
/**
 * The reader wrote it: its author's account is one of the identity's.
 */
mine: boolean, 
/**
 * The body names the reader (R-5.3), by [`mentions`]'s rule.
 *
 * `@name` or Jira's `[~name]`, for an account `@me` resolves to.
 */
mentions_me: boolean, };

/**
 * What a value means to a reader, in the shell's five words.
 *
 * * `ok`: done and good (a merged pull request, a passing build).
 * * `warn`: needs a look, nothing has failed yet.
 * * `err`: failed (a failing build).
 * * `info`: in motion or open (an open pull request, a running build).
 * * `muted`: over and of no further interest (a closed pull request).
 *
 * Serialized lowercase, the spelling the TypeScript mirror and a declared
 * path's value map both use.
 *
 * The adapter says which; the shell owns the colours.
 */
export type Tone = "ok" | "warn" | "err" | "info" | "muted";

/**
 * One entry of a [`Block::Tree`].
 */
export type TreeEntry = { name: string, kind: EntryKind, 
/**
 * A file's size in bytes; `None` for a directory or a submodule.
 */
size: number | null, 
/**
 * The section that opens it in the same tab; `None` for one that cannot
 * be opened here (a submodule is another repository).
 */
section: string | null, };

/**
 * The answer the checkout panel draws: a state, or why there is none.
 *
 * None of these is an error. A checkout panel is worth drawing without a
 * state, and each arm is a sentence the panel says.
 *
 * - `read`: git answered.
 * - `no_git`: No `git` on `PATH`.
 * - `not_a_repo`: The path is not a git working tree, or not there at all.
 * - `failed`: git ran and did not answer: it timed out, or refused with
 *   `reason`.
 *
 * Issue #704.
 */
export type TreeRead = { "status": "read" } & WorkingTreeState | { "status": "no_git" } | { "status": "not_a_repo" } | { "status": "failed", reason: string, };

/**
 * What is uncommitted at a checkout.
 *
 * Issue #824, ADR-0026.
 */
export type UncommittedDiff = { 
/**
 * Changed in the working tree and not staged (`git diff`).
 */
unstaged: CappedDiff, 
/**
 * Staged and not committed (`git diff --cached`).
 */
staged: CappedDiff, 
/**
 * Paths git does not track and does not ignore, in git's order; a new
 * directory once, with its trailing `/`.
 */
untracked: Array<string>, 
/**
 * More than [`MAX_UNTRACKED`] paths were untracked; the rest are not
 * listed.
 */
untracked_truncated: boolean, };

/**
 * What a pasted URL resolved to: the entity, and the kind that says where it
 * opens (issue #496).
 *
 * Deliberately not an [`EntityRow`] and not an [`EntityDetail`]. A paste
 * answers one question -- *which entity is this link* -- and the frontend
 * then navigates to that entity's own address, at which point the detail is
 * fetched by the read that already exists. Answering with a row here would
 * make every paste pay for a title and a path nothing on the way draws, and
 * would put a second statement in the tree that has to keep in step with
 * `sync.live_item`'s columns.
 *
 * `kind` rather than only the id because an entity id names a *namespace*
 * and a key (`gitea:acme/svc#142`), not a kind, and the address an entity
 * opens at is `#/<kind>/<id>` -- or, for an asset or a route, the Tree's own
 * encoder. The frontend has one function that turns the pair into an
 * address; it has nothing that turns an id alone into one.
 */
export type UrlMatch = { entity_id: string, kind: string, };

/**
 * One write a view offers: a toolbar button ([`SourceView::actions`]) or one
 * in a [`Block::Actions`].
 *
 * **The source declares the dialog** (issue #702): its `fields`, its
 * sentence, its tone. The shell draws any action's dialog without knowing
 * which op it is, hands the values the reader gave, by field `name`, to its
 * table of payloads (`app/src/lib/detail/view/actions.ts`, one entry per op),
 * and draws no action whose op has no entry there or that the source's
 * descriptor does not declare.
 *
 * An action with no fields, no sentence, no `with_settings` and not
 * `danger` queues on the button, with no dialog; its fixed `values` go with
 * it.
 *
 * Issues #667 and #702.
 */
export type ViewAction = { 
/**
 * The [`WriteOp`](crate::WriteOp) identifier the action queues.
 */
write_op: string, 
/**
 * The button's words, in the reader's language (`"Merge…"`). The
 * dialog's own button is the label without its ellipsis.
 */
label: string, 
/**
 * What the dialog asks for, in order. Empty when the op takes nothing.
 */
fields: Array<ActionField>, 
/**
 * The dialog's sentence, asking the reader to confirm.
 */
confirm: string | null, 
/**
 * The entity's main write here (*Merge…* on a pull request): drawn filled.
 */
primary: boolean, 
/**
 * A write that destroys something (*Delete branch…*): drawn in the error
 * tone, and always through its dialog, never on the button.
 */
danger: boolean, 
/**
 * Why the action cannot be taken right now, in the reader's language
 * (*Not mergeable: …*). `Some` draws the button disabled with this as
 * its tooltip and accessible description, so the reader learns why
 * rather than wondering where the button went. **The source's answer**,
 * read in the adapter; the shell never decides it (ADR-0007).
 */
disabled: string | null, 
/**
 * The dialog also offers the entity's own settings -- the view's
 * [`Block::Settings`] of this entity -- to change for this one write,
 * and sends only those the reader changed (a run's parameter overrides,
 * #669).
 */
with_settings: boolean, 
/**
 * Answers the source fixes, by the name the op's payload reads them
 * under: never drawn, handed to the shell's payload table with the
 * dialog's values (issue #705). What lets one op be two actions --
 * *Approve…* and *Request changes…* are both `submit_review`, told apart
 * by `verdict` -- without the shell reading the label. A name here is
 * never also a field's.
 */
values: Record<string, string>, 
/**
 * The entity the write targets where it is not the view's own, as an
 * entity id (issue #707): a branch's *New branch…* and *Open pull
 * request…* are created in the branch's **repository**, the container
 * those ops target. `None` targets the entity the view is of, as every
 * action did before. The shell never derives it from the view's id; it
 * takes this one as given, as it takes a [`Block::Settings`]' `entity`.
 * Absent on the wire reads as `None`.
 */
entity: string | null, 
/**
 * Whether the action sends the reader's pending line comments on the
 * entity's diff (issue #709; [`Block::Diff`]'s `commentable`). Absent on
 * the wire reads as [`LineComments::None`].
 */
line_comments: LineComments, };

/**
 * One typed piece of a section. The vocabulary the shell draws, and nothing
 * else.
 *
 * Internally tagged (`{"type": "diff", ...}`) so the TypeScript mirror is a
 * discriminated union on `type`.
 *
 * - `facts`: Label and value pairs: a commit's sha and author, a build's
 *   outcome.
 * - `text`: Prose: a commit message, a pull request's description. `markdown`
 *   says the source writes Markdown there; the shell still draws it as text,
 *   never as markup (ADR-0020 rule 5).
 * - `entities`: A list of things, each of which may be an address (ADR-0020
 *   rule 7): a commit's parents, a pull request's commits, a build's changes.
 * - `diff`: A diff, parsed by [`crate::diff`] and capped there. `truncated`
 *   says the per-view cap cut it: some file after the cut carries its counts
 *   and no lines.
 *
 *   `commentable` says the reader may write line comments on it (issue #709):
 *   a pull request's diff, while a review can still be given. The shell keeps
 *   what the reader writes as pending comments of the entity the view is of,
 *   and they go with the toolbar actions that take them
 *   ([`ViewAction::line_comments`]). `None` -- a commit's diff, a closed pull
 *   request's -- draws no way to comment. Absent on the wire reads as `None`.
 * - `log`: Plain text with meaningful lines, drawn monospace: a build log's
 *   tail. `truncated` says this is the end of something longer.
 * - `settings`: Named values a reader may be allowed to change (ADR-0020 rule
 *   4): a build configuration's parameters. `entity` is what an edit targets
 *   and `write_op` is the [`WriteOp`](crate::WriteOp) identifier that changes
 *   a value; a view never writes.
 * - `actions`: Writes a reader can start from this section, one button each
 *   (issue #667, ADR-0020 rule 4). An action of the whole entity belongs in
 *   [`SourceView::actions`], the toolbar (issue #702); this block is for one
 *   that belongs to one section, and never repeats a toolbar action.
 *
 *   Whether an action can be taken is the source's answer, read in the adapter
 *   ([`ViewAction::disabled`]); the shell never decides it by reading a fact's
 *   label or the payload (ADR-0007). The shell additionally draws an action
 *   only where the source's descriptor declares its `write_op`, and queues it
 *   through the write queue against the entity the view is of. A view never
 *   writes.
 * - `tree`: One directory of a tree of files (issue #711): a repository's at a
 *   branch. `trail` leads from the tree's top to this directory; each entry
 *   that can be opened names the section that opens it.
 *
 *   **A section a block names opens in the tab it is drawn in**: the shell
 *   reads it with `view(entity, section)` as it reads a deferred tab, and
 *   draws the answer in place of this block's section, under the same tab. So
 *   walking a tree is one tab whose content changes, and the ids are the
 *   source's own (`files:src/lib`); the shell never parses them.
 *
 *   `truncated` says the source's cap cut the listing (Gitea's adapter keeps
 *   1,000 entries), and `url` is the directory's page at the source, where the
 *   rest is.
 * - `file`: One file of a tree (issue #711), drawn monospace with line
 *   numbers. `trail` leads from the tree's top to the file, the file last.
 *
 *   `text` is empty for a `binary` file. `truncated` says a cap cut the text
 *   (the source's own: Gitea's adapter keeps 1 MB and 10,000 lines), and `url`
 *   is the file's page at the source, where the rest is. `language` is a hint
 *   from the file's name (`"rust"`, `"markdown"`), `None` where the source has
 *   none; the shell may draw a `"markdown"` file rendered, with the raw text a
 *   toggle away, and never highlights.
 *
 * Named `ViewBlock` in the generated TypeScript (#1326), because the shell's
 * barrel already exports a time-tracking `Block`.
 */
export type ViewBlock = { "type": "facts", facts: Array<Fact>, } | { "type": "text", text: string, markdown: boolean, } | { "type": "entities", title: string | null, rows: Array<ViewRow>, } | { "type": "diff", files: Array<DiffFile>, truncated: boolean, commentable: Commenting | null, 
/**
 * The threads on files this diff no longer holds (issue #709): each
 * outdated, drawn in a list at the end of the diff. Absent on the
 * wire reads as empty.
 */
gone: Array<FileThread>, } | { "type": "log", text: string, truncated: boolean, } | { "type": "settings", title: string | null, entity: string, write_op: string, values: Array<Setting>, } | { "type": "actions", actions: Array<ViewAction>, } | { "type": "tree", trail: Array<Crumb>, entries: Array<TreeEntry>, 
/**
 * The source's cap cut the listing; the rest is at `url`.
 */
truncated: boolean, 
/**
 * The directory's page at its source.
 */
url: string | null, } | { "type": "file", trail: Array<Crumb>, path: string, 
/**
 * Empty for a binary file.
 */
text: string, 
/**
 * A hint from the file's name (`"rust"`, `"markdown"`); nothing is
 * highlighted.
 */
language: string | null, 
/**
 * The file's size in bytes, as the source reports it.
 */
size: number, 
/**
 * A cap cut the text; the rest is at `url`.
 */
truncated: boolean, binary: boolean, 
/**
 * The file's page at its source.
 */
url: string | null, 
/**
 * The file's blob sha at the ref drawn (#883): the version an edit
 * of it is made against, `WriteOp::CommitFile`'s `base_sha`. `None`
 * where the source has none; absent on the wire reads as `None`.
 */
sha: string | null, };

/**
 * The payload of the shell's `view:fresh` event (#2242): a view the shell
 * was answered from memory was read again behind, and its answer moved, so
 * a page showing it reads it again (from memory, now the fresh answer).
 */
export type ViewFresh = { entity_id: string, section: string | null, };

/**
 * One row of an [`Block::Entities`] list.
 *
 * `entity_id` is an address only where the mirror holds that entity: the
 * adapter names what it would be, and the app clears it for an entity the
 * mirror does not hold, so the shell draws that row as plain text (ADR-0020
 * rule 7). `url` is the source's own page for the row, when it has one.
 */
export type ViewRow = { entity_id: string | null, title: string, 
/**
 * A quiet second reading: a short sha, a time, a branch.
 */
meta: string | null, tone: Tone | null, 
/**
 * A short state word drawn as a chip (`merged`, `failed`), coloured by
 * `tone`.
 */
chip: string | null, url: string | null, };

/**
 * One tab of a view.
 *
 * A **deferred** section is expensive to read -- a pull request's files, a
 * build's log. The whole-view read carries its `id`, `title` and `count` with
 * `blocks` empty; the shell reads its blocks when the reader first opens the
 * tab, by calling `view` again with the section's `id` (ADR-0020 rule 3).
 */
export type ViewSection = { 
/**
 * Stable within one kind's view (`"overview"`, `"diff"`): what a deferred
 * read names. Unique within one view.
 */
id: string, 
/**
 * The tab's label, in the reader's language (`"Diff"`).
 */
title: string, 
/**
 * What the tab counts, drawn beside its title -- files changed, failed
 * tests. `None` draws no count.
 */
count: number | null, 
/**
 * Whether `blocks` is read on first open rather than with the view.
 */
deferred: boolean, blocks: Array<ViewBlock>, };

/**
 * What a checkout's working tree is doing right now.
 *
 * Issue #704.
 */
export type WorkingTreeState = { 
/**
 * The branch checked out; `None` for a detached head.
 */
branch: string | null, 
/**
 * The commit checked out; `None` in a repository with no commit yet.
 */
head: string | null, 
/**
 * The upstream the branch tracks, as git names it (`origin/main`).
 */
upstream: string | null, 
/**
 * Commits on the branch its upstream does not have: *to push*. Zero
 * without an upstream, or with one whose ref is not on disk.
 */
ahead: number, 
/**
 * Commits on the upstream the branch does not have: *to pull*.
 */
behind: number, 
/**
 * Tracked paths that differ from `HEAD`, staged or not, renames as one.
 */
changed: number, 
/**
 * Paths git does not track and does not ignore. A new directory counts
 * once, however many files it holds (`--untracked-files=normal`).
 */
untracked: number, 
/**
 * Paths with an unresolved merge conflict.
 */
conflicted: number, };

/**
 * One write a surface would offer: `op` on `entity`, or one field of it
 * ([`field`](Self::field), #2055).
 *
 * `entity` is an [`EntityRef`](knobas_spi_types::entity::EntityRef) in
 * string form, the address every [`WriteOp`](crate::WriteOp) carries. For a
 * create it is the container the write acts in (a project, a space), the
 * entity the create's own payload names.
 */
export type WriteAsk = { entity: string, 
/**
 * A [`WriteOp`](crate::WriteOp) identifier, as `write_ops` spells it.
 */
op: string, 
/**
 * One field of the op's payload, by the name the payload carries it
 * under (`parameters`, `first_in_queue` on a `run_build`): may the
 * write carry it set? `None` asks about the write itself (#2055).
 *
 * A field can need more than its write: a TeamCity user may run a
 * build and not override its parameters for that run, or not put it
 * first in the queue. Such an ask is answered by what the field needs
 * beyond the write. An adapter that tells no field apart answers it as
 * it answers the write, which is still true: a write the credential
 * cannot make carries no field.
 */
field?: string, };
