//! The source-view vocabulary: what an adapter draws for one of its own kinds
//! when a detail opens (ADR-0020, `CONTEXT.md` **Source view**).
//!
//! A view is a list of [`ViewSection`]s, each a list of typed [`Block`]s from
//! one fixed vocabulary -- facts, text, an entity list, a diff, a log,
//! settings, actions, a directory and a file. The shell draws the vocabulary and never learns which adapter
//! drew a block, which is what keeps spec §3a's promise: a GitLab adapter that
//! answers with a diff block gets the diff viewer on day one.
//!
//! **Frozen once landed** (issue #661). Adding a block, a field or a tone is a
//! §10.8 entry, as adding a [`WriteOp`](crate::WriteOp) variant is.
//!
//! Every type here is plain serde data (spec §3a), so an out-of-process
//! adapter can draw views too. Every string is **source text**: the shell
//! interpolates it and never renders it as markup (ADR-0020 rule 5).

pub use crate::Tone;
pub use crate::diff::{
    DiffChange, DiffFile, DiffLine, DiffLineKind, FileThread, Hunk, LineComment, LineThread,
};

/// What a view's answer stands at, as
/// [`Source::view_standing`](crate::Source::view_standing) says before the
/// read (ADR-0032, #2239). Never on the wire: it is the app's to act on, and
/// the shell draws a kept answer as it draws a fresh one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Standing {
    /// Read live every time, and kept nowhere: the default.
    Live,
    /// Fixed by the entity and section alone -- something at a full commit
    /// sha -- so the app keeps it in memory for the session and answers the
    /// same pair again from there.
    Fixed,
    /// Keyed by a reference that moves -- a branch name, a branch list, a
    /// tree by branch -- so the app answers the pair from memory where it
    /// holds it and reads it again behind the answer; the next ask gets the
    /// fresh one (#2242). A re-read that fails leaves the kept answer.
    Moving,
}

/// What [`Source::view`](crate::Source::view) answers for one entity: its
/// sections, in the order the shell draws them as tabs. The first is the one a
/// detail opens on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct SourceView {
    pub sections: Vec<ViewSection>,
    /// What a reader can do to the entity, drawn as a toolbar between the
    /// detail's header and the tabs, on every tab (issue #702, spec #701).
    /// Empty draws no toolbar. Absent on the wire reads as empty, so a view
    /// drawn before the toolbar decodes.
    ///
    /// A [`Block::Actions`] is for an action that belongs to one section; an
    /// action is in one place or the other, never both.
    #[serde(default)]
    pub actions: Vec<ViewAction>,
}

impl SourceView {
    /// A view of `sections` with no toolbar: what a section read answers, and
    /// any view whose entity offers no action.
    #[must_use]
    pub fn new(sections: Vec<ViewSection>) -> Self {
        Self {
            sections,
            actions: Vec::new(),
        }
    }

    /// Draw every write `refused` answers a reason for as one the reader
    /// cannot take, wherever the view offers it (issue #2030): an action is
    /// disabled with the reason, a settings block's values stop being
    /// editable, and a diff's line comments are no longer offered.
    ///
    /// For an adapter that learns what its **credential** may write -- a
    /// token without the write scope -- after drawing what the entity
    /// offers. A token can be replaced, so the reader can still reach the
    /// state where the write is enabled, and ADR-0020 rule 4 draws such a
    /// write disabled with its reason rather than leaving it out. The reason
    /// replaces any the adapter drew: whatever else the reader fixes, the
    /// credential still cannot make the write.
    ///
    /// `refused` is asked once for each write the view offers, in the order
    /// they are drawn, so an adapter can also collect what to ask about.
    pub fn refuse_writes(&mut self, mut refused: impl FnMut(&str) -> Option<String>) {
        fn disable(actions: &mut [ViewAction], refused: &mut impl FnMut(&str) -> Option<String>) {
            for action in actions {
                if let Some(reason) = refused(&action.write_op) {
                    action.disabled = Some(reason);
                }
            }
        }
        disable(&mut self.actions, &mut refused);
        for block in self.sections.iter_mut().flat_map(|s| &mut s.blocks) {
            // No wildcard arm: a block that comes to carry a write is a
            // decision here, not one this walk misses.
            match block {
                Block::Actions { actions } => disable(actions, &mut refused),
                Block::Settings {
                    write_op, values, ..
                } => {
                    if refused(write_op).is_some() {
                        for value in values {
                            value.editable = false;
                        }
                    }
                }
                Block::Diff { commentable, .. } => {
                    // `Commenting` carries no reason to draw, so the line
                    // comments go; the review actions beside them say why.
                    if commentable
                        .as_ref()
                        .is_some_and(|c| refused(&c.write_op).is_some())
                    {
                        *commentable = None;
                    }
                    // A reply and a resolve go the same way: dropped where
                    // the credential cannot make them.
                    if let Some(c) = commentable.as_mut() {
                        for op in [&mut c.reply, &mut c.resolve] {
                            if op.as_deref().is_some_and(|op| refused(op).is_some()) {
                                *op = None;
                            }
                        }
                    }
                }
                Block::Facts { .. }
                | Block::Text { .. }
                | Block::Entities { .. }
                | Block::Log { .. }
                | Block::Tree { .. }
                | Block::File { .. } => {}
            }
        }
    }
}

/// One tab of a view.
///
/// A **deferred** section is expensive to read -- a pull request's files, a
/// build's log. The whole-view read carries its `id`, `title` and `count` with
/// `blocks` empty; the shell reads its blocks when the reader first opens the
/// tab, by calling `view` again with the section's `id` (ADR-0020 rule 3).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ViewSection {
    /// Stable within one kind's view (`"overview"`, `"diff"`): what a deferred
    /// read names. Unique within one view.
    pub id: String,
    /// The tab's label, in the reader's language (`"Diff"`).
    pub title: String,
    /// What the tab counts, drawn beside its title -- files changed, failed
    /// tests. `None` draws no count.
    pub count: Option<u64>,
    /// Whether `blocks` is read on first open rather than with the view.
    pub deferred: bool,
    pub blocks: Vec<Block>,
}

/// One typed piece of a section. The vocabulary the shell draws, and nothing
/// else.
///
/// Internally tagged (`{"type": "diff", ...}`) so the TypeScript mirror is a
/// discriminated union on `type`.
///
/// - `facts`: Label and value pairs: a commit's sha and author, a build's
///   outcome.
/// - `text`: Prose: a commit message, a pull request's description. `markdown`
///   says the source writes Markdown there; the shell still draws it as text,
///   never as markup (ADR-0020 rule 5).
/// - `entities`: A list of things, each of which may be an address (ADR-0020
///   rule 7): a commit's parents, a pull request's commits, a build's changes.
/// - `diff`: A diff, parsed by [`crate::diff`] and capped there. `truncated`
///   says the per-view cap cut it: some file after the cut carries its counts
///   and no lines.
///
///   `commentable` says the reader may write line comments on it (issue #709):
///   a pull request's diff, while a review can still be given. The shell keeps
///   what the reader writes as pending comments of the entity the view is of,
///   and they go with the toolbar actions that take them
///   ([`ViewAction::line_comments`]). `None` -- a commit's diff, a closed pull
///   request's -- draws no way to comment. Absent on the wire reads as `None`.
/// - `log`: Plain text with meaningful lines, drawn monospace: a build log's
///   tail. `truncated` says this is the end of something longer.
/// - `settings`: Named values a reader may be allowed to change (ADR-0020 rule
///   4): a build configuration's parameters. `entity` is what an edit targets
///   and `write_op` is the [`WriteOp`](crate::WriteOp) identifier that changes
///   a value; a view never writes.
/// - `actions`: Writes a reader can start from this section, one button each
///   (issue #667, ADR-0020 rule 4). An action of the whole entity belongs in
///   [`SourceView::actions`], the toolbar (issue #702); this block is for one
///   that belongs to one section, and never repeats a toolbar action.
///
///   Whether an action can be taken is the source's answer, read in the adapter
///   ([`ViewAction::disabled`]); the shell never decides it by reading a fact's
///   label or the payload (ADR-0007). The shell additionally draws an action
///   only where the source's descriptor declares its `write_op`, and queues it
///   through the write queue against the entity the view is of. A view never
///   writes.
/// - `tree`: One directory of a tree of files (issue #711): a repository's at a
///   branch. `trail` leads from the tree's top to this directory; each entry
///   that can be opened names the section that opens it.
///
///   **A section a block names opens in the tab it is drawn in**: the shell
///   reads it with `view(entity, section)` as it reads a deferred tab, and
///   draws the answer in place of this block's section, under the same tab. So
///   walking a tree is one tab whose content changes, and the ids are the
///   source's own (`files:src/lib`); the shell never parses them.
///
///   `truncated` says the source's cap cut the listing (Gitea's adapter keeps
///   1,000 entries), and `url` is the directory's page at the source, where the
///   rest is.
/// - `file`: One file of a tree (issue #711), drawn monospace with line
///   numbers. `trail` leads from the tree's top to the file, the file last.
///
///   `text` is empty for a `binary` file. `truncated` says a cap cut the text
///   (the source's own: Gitea's adapter keeps 1 MB and 10,000 lines), and `url`
///   is the file's page at the source, where the rest is. `language` is a hint
///   from the file's name (`"rust"`, `"markdown"`), `None` where the source has
///   none; the shell may draw a `"markdown"` file rendered, with the raw text a
///   toggle away, and never highlights.
///
/// Named `ViewBlock` in the generated TypeScript (#1326), because the shell's
/// barrel already exports a time-tracking `Block`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export_to = "entity.ts", rename = "ViewBlock")]
pub enum Block {
    Facts {
        facts: Vec<Fact>,
    },
    Text {
        text: String,
        markdown: bool,
    },
    Entities {
        title: Option<String>,
        rows: Vec<ViewRow>,
    },
    Diff {
        files: Vec<DiffFile>,
        truncated: bool,
        #[serde(default)]
        commentable: Option<Commenting>,
        /// The threads on files this diff no longer holds (issue #709): each
        /// outdated, drawn in a list at the end of the diff. Absent on the
        /// wire reads as empty.
        #[serde(default)]
        gone: Vec<FileThread>,
    },
    Log {
        text: String,
        truncated: bool,
    },
    Settings {
        title: Option<String>,
        entity: String,
        write_op: String,
        values: Vec<Setting>,
    },
    Actions {
        actions: Vec<ViewAction>,
    },
    Tree {
        trail: Vec<Crumb>,
        entries: Vec<TreeEntry>,
        /// The source's cap cut the listing; the rest is at `url`.
        truncated: bool,
        /// The directory's page at its source.
        url: Option<String>,
    },
    File {
        trail: Vec<Crumb>,
        path: String,
        /// Empty for a binary file.
        text: String,
        /// A hint from the file's name (`"rust"`, `"markdown"`); nothing is
        /// highlighted.
        language: Option<String>,
        /// The file's size in bytes, as the source reports it.
        size: u64,
        /// A cap cut the text; the rest is at `url`.
        truncated: bool,
        binary: bool,
        /// The file's page at its source.
        url: Option<String>,
        /// The file's blob sha at the ref drawn (#883): the version an edit
        /// of it is made against, `WriteOp::CommitFile`'s `base_sha`. `None`
        /// where the source has none; absent on the wire reads as `None`.
        #[serde(default)]
        sha: Option<String>,
        /// The commit the file was read at (#2256): what
        /// [`Source::file_bytes`](crate::Source::file_bytes) takes back to
        /// read this file's bytes, or a file beside it (a README's relative
        /// image), at the same commit. The source's own id, never parsed by
        /// the shell; `None` where the source has none, and the byte read
        /// then reads at the entity's moving reference. Absent on the wire
        /// reads as `None`.
        #[serde(default)]
        commit: Option<String>,
    },
}

/// The most bytes one [`Source::file_bytes`](crate::Source::file_bytes)
/// answers (#2256, spec #2234 step 5): 50 MiB. A file over it is refused with
/// its size ([`FileBytes::TooLarge`]), never cut.
pub const BYTES_CAP: u64 = 50 * 1024 * 1024;

/// What [`Source::file_bytes`](crate::Source::file_bytes) answers for one
/// file (#2256). Never on the wire as data: the app hands the bytes to the
/// shell raw and a refusal as an error naming the size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileBytes {
    /// The whole file, at most the cap asked for.
    Read(Vec<u8>),
    /// The file is longer than the cap asked for; this is its size in bytes,
    /// and none of it was kept.
    TooLarge { size: u64 },
}

/// One step of a [`Block::Tree`]'s or [`Block::File`]'s trail: a
/// directory's (or the file's own) name and the section that opens it.
///
/// Issue #711. A section a block names opens **in the tab it is drawn in**,
/// read with `entity_view(entity, section)`; the id is the source's and never
/// parsed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Crumb {
    pub name: String,
    pub section: String,
}

/// One entry of a [`Block::Tree`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct TreeEntry {
    pub name: String,
    pub kind: EntryKind,
    /// A file's size in bytes; `None` for a directory or a submodule.
    pub size: Option<u64>,
    /// The section that opens it in the same tab; `None` for one that cannot
    /// be opened here (a submodule is another repository).
    pub section: Option<String>,
}

/// What a [`TreeEntry`] is, as git records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum EntryKind {
    Dir,
    File,
    Symlink,
    Submodule,
}

/// One write a view offers: a toolbar button ([`SourceView::actions`]) or one
/// in a [`Block::Actions`].
///
/// **The source declares the dialog** (issue #702): its `fields`, its
/// sentence, its tone. The shell draws any action's dialog without knowing
/// which op it is, hands the values the reader gave, by field `name`, to its
/// table of payloads (`app/src/lib/detail/view/actions.ts`, one entry per op),
/// and draws no action whose op has no entry there or that the source's
/// descriptor does not declare.
///
/// An action with no fields, no sentence, no `with_settings` and not
/// `danger` queues on the button, with no dialog; its fixed `values` go with
/// it.
///
/// Issues #667 and #702.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ViewAction {
    /// The [`WriteOp`](crate::WriteOp) identifier the action queues.
    pub write_op: String,
    /// The button's words, in the reader's language (`"Merge…"`). The
    /// dialog's own button is the label without its ellipsis.
    pub label: String,
    /// What the dialog asks for, in order. Empty when the op takes nothing.
    #[serde(default)]
    pub fields: Vec<ActionField>,
    /// The dialog's sentence, asking the reader to confirm.
    pub confirm: Option<String>,
    /// The entity's main write here (*Merge…* on a pull request): drawn filled.
    #[serde(default)]
    pub primary: bool,
    /// A write that destroys something (*Delete branch…*): drawn in the error
    /// tone, and always through its dialog, never on the button.
    #[serde(default)]
    pub danger: bool,
    /// Why the action cannot be taken right now, in the reader's language
    /// (*Not mergeable: …*). `Some` draws the button disabled with this as
    /// its tooltip and accessible description, so the reader learns why
    /// rather than wondering where the button went. **The source's answer**,
    /// read in the adapter; the shell never decides it (ADR-0007).
    #[serde(default)]
    pub disabled: Option<String>,
    /// The dialog also offers the entity's own settings -- the view's
    /// [`Block::Settings`] of this entity -- to change for this one write,
    /// and sends only those the reader changed (a run's parameter overrides,
    /// #669).
    #[serde(default)]
    pub with_settings: bool,
    /// Answers the source fixes, by the name the op's payload reads them
    /// under: never drawn, handed to the shell's payload table with the
    /// dialog's values (issue #705). What lets one op be two actions --
    /// *Approve…* and *Request changes…* are both `submit_review`, told apart
    /// by `verdict` -- without the shell reading the label. A name here is
    /// never also a field's.
    #[serde(default)]
    #[ts(type = "Record<string, string>")]
    pub values: std::collections::BTreeMap<String, String>,
    /// The entity the write targets where it is not the view's own, as an
    /// entity id (issue #707): a branch's *New branch…* and *Open pull
    /// request…* are created in the branch's **repository**, the container
    /// those ops target. `None` targets the entity the view is of, as every
    /// action did before. The shell never derives it from the view's id; it
    /// takes this one as given, as it takes a [`Block::Settings`]' `entity`.
    /// Absent on the wire reads as `None`.
    #[serde(default)]
    pub entity: Option<String>,
    /// Whether the action sends the reader's pending line comments on the
    /// entity's diff (issue #709; [`Block::Diff`]'s `commentable`). Absent on
    /// the wire reads as [`LineComments::None`].
    #[serde(default)]
    pub line_comments: LineComments,
}

/// What a [`ViewAction`] does with the reader's pending line comments (issue
/// #709). The comments are the shell's -- written on a commentable
/// [`Block::Diff`] and kept until sent or discarded -- and the action only
/// says whether they go with it.
///
/// - `none`: The action never sends them (*Comment…*, *Merge…*).
/// - `sends`: The action sends them when there are some, and is drawn either
///   way (*Approve…*, *Request changes…*): its dialog lists what goes with it.
/// - `requires`: The action sends them and is drawn only while there are some
///   (*Submit review…*, a review that is the comments and nothing else).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS,
)]
#[serde(rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum LineComments {
    #[default]
    None,
    Sends,
    Requires,
}

/// How a reader comments on a [`Block::Diff`]'s lines (issue #709).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Commenting {
    /// The [`WriteOp`](crate::WriteOp) identifier the comments are sent
    /// through (`submit_review`). The shell offers to comment only where the
    /// source's descriptor declares it and the shell can queue it, as for an
    /// action.
    pub write_op: String,
    /// The identifier a reply to one of the diff's threads goes through
    /// (`reply_to_thread`, #2277), on the threads that say they take one
    /// ([`LineThread::replies`](crate::diff::LineThread::replies)); `None`
    /// offers no reply. Absent on the wire reads as `None`.
    #[serde(default)]
    pub reply: Option<String>,
    /// The identifier resolving or reopening a thread goes through
    /// (`resolve_thread`, #2277), on every thread with an id; `None` where
    /// the source cannot resolve threads (Forgejo 17 has no endpoint for it).
    /// Absent on the wire reads as `None`.
    #[serde(default)]
    pub resolve: Option<String>,
    /// The head commit the diff's line comments are written against, in the
    /// source's spelling (#2276): the shell keeps the reader's unsent
    /// comments by pull request and this commit. `None` where the source did
    /// not say. Absent on the wire reads as `None`.
    #[serde(default)]
    pub head: Option<String>,
}

impl ViewAction {
    /// An action that queues `write_op` on the button: no fields, no
    /// sentence, no tone. Struct-update syntax adds the rest.
    #[must_use]
    pub fn new(write_op: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            write_op: write_op.into(),
            label: label.into(),
            fields: Vec::new(),
            confirm: None,
            primary: false,
            danger: false,
            disabled: None,
            with_settings: false,
            values: std::collections::BTreeMap::new(),
            entity: None,
            line_comments: LineComments::None,
        }
    }
}

/// One input of a [`ViewAction`]'s dialog. The reader's answer reaches the
/// shell's payload table as text under `name`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ActionField {
    /// The key the value is handed on under, in the op's own words
    /// (`"style"`, `"delete_branch"`). Unique within one action.
    pub name: String,
    /// The field's label, in the reader's language.
    pub label: String,
    /// How it asks: one line, several, one of a few, or a checkbox.
    pub kind: FieldKind,
    /// What the field starts with: a text's prefill, a choice's preselected
    /// option (`None` picks the first), a flag's `"true"` (`None` or anything
    /// else starts unchecked). A flag answers `"true"` or `"false"`.
    #[serde(default)]
    pub value: Option<String>,
    /// The dialog will not send without it: a text with something other than
    /// blanks in it, a choice with an option picked, a flag checked.
    #[serde(default)]
    pub required: bool,
    /// What an empty text field says (`"default branch"`).
    #[serde(default)]
    pub placeholder: Option<String>,
}

impl ActionField {
    /// A field with no prefill, not required, with no placeholder.
    #[must_use]
    pub fn new(name: impl Into<String>, label: impl Into<String>, kind: FieldKind) -> Self {
        Self {
            name: name.into(),
            label: label.into(),
            kind,
            value: None,
            required: false,
            placeholder: None,
        }
    }
}

/// How an [`ActionField`] asks. Internally tagged (`{"type": "choice", ...}`),
/// a union on `type` in the TypeScript mirror as [`Block`] is.
///
/// - `text`: One line. `suggestions` are offered as the reader types and none
///   is picked: a run's branch suggests the branches recent builds ran on.
/// - `long_text`: Several lines of Markdown: a description, a comment. Drawn as
///   a plain text area; the shell never renders it (ADR-0020 rule 5).
/// - `choice`: One of `options`, picked.
/// - `flag`: A checkbox.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export_to = "entity.ts")]
pub enum FieldKind {
    Text {
        #[serde(default)]
        suggestions: Vec<String>,
    },
    LongText,
    Choice {
        options: Vec<ChoiceOption>,
    },
    Flag,
}

/// One option of a [`FieldKind::Choice`]: the `value` the op takes, and the
/// `label` the reader sees.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ChoiceOption {
    /// What the op takes when this option is picked (`"squash"`).
    pub value: String,
    /// What the reader sees (`"Squash"`).
    pub label: String,
}

impl ChoiceOption {
    /// The option whose answer is `value`, shown as `label`.
    #[must_use]
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

/// One label and its value.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Fact {
    pub label: String,
    pub value: String,
    /// Colour, only where something is wrong or moving (the Signal design).
    pub tone: Option<Tone>,
}

/// One row of an [`Block::Entities`] list.
///
/// `entity_id` is an address only where the mirror holds that entity: the
/// adapter names what it would be, and the app clears it for an entity the
/// mirror does not hold, so the shell draws that row as plain text (ADR-0020
/// rule 7). `url` is the source's own page for the row, when it has one.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct ViewRow {
    pub entity_id: Option<String>,
    pub title: String,
    /// A quiet second reading: a short sha, a time, a branch.
    pub meta: Option<String>,
    pub tone: Option<Tone>,
    /// A short state word drawn as a chip (`merged`, `failed`), coloured by
    /// `tone`.
    pub chip: Option<String>,
    pub url: Option<String>,
    /// A commit another source may hold, named by where it lives rather than
    /// by an id (issue #665): a TeamCity change knows its sha and its VCS
    /// root's URL, and cannot see a Gitea mirror. **The app's to resolve, not
    /// the shell's to draw**: `entity_view` looks the commit's page up with
    /// the paste resolver and, where the mirror holds it, sets `entity_id`;
    /// either way it clears this field, so it never crosses to the shell.
    /// Absent on the wire when `None`, so a row without one is the shape it
    /// was before.
    ///
    /// Not in the generated TypeScript (`#[ts(skip)]`, #1326): it never reaches
    /// the shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(skip)]
    pub commit: Option<CommitHint>,
}

/// Where a commit lives, for [`ViewRow::commit`]: a git repository's URL as
/// the source knows it (a clone URL, `.git` and all, with no credential) and
/// the commit's full sha.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommitHint {
    pub repo_url: String,
    pub sha: String,
}

/// One named value of a [`Block::Settings`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export_to = "entity.ts")]
pub struct Setting {
    pub name: String,
    /// The value as the source holds it. Empty for a `secret` one: the
    /// adapter never sends a password's value to the shell.
    pub value: String,
    /// Set by something above this entity (a TeamCity project's parameter
    /// seen on a configuration).
    pub inherited: bool,
    pub secret: bool,
    /// Whether the block's `write_op` may change it.
    pub editable: bool,
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One probe per field kind, and an action with every flag set. The match
    /// on [`FieldKind`] has no wildcard arm, so a new kind stops this module
    /// compiling until it has a probe here.
    fn every_action() -> Vec<ViewAction> {
        let fields = vec![
            ActionField {
                placeholder: Some("default branch".into()),
                ..ActionField::new(
                    "branch",
                    "Branch",
                    FieldKind::Text {
                        suggestions: vec!["feature/x".into()],
                    },
                )
            },
            ActionField {
                value: Some("Opened from knobas".into()),
                required: true,
                ..ActionField::new("body", "Description", FieldKind::LongText)
            },
            ActionField {
                value: Some("squash".into()),
                ..ActionField::new(
                    "style",
                    "Style",
                    FieldKind::Choice {
                        options: vec![
                            ChoiceOption::new("merge", "Merge commit"),
                            ChoiceOption::new("squash", "Squash"),
                        ],
                    },
                )
            },
            ActionField {
                value: Some("true".into()),
                ..ActionField::new("delete_branch", "Delete the branch", FieldKind::Flag)
            },
        ];
        for field in &fields {
            match field.kind {
                FieldKind::Text { .. }
                | FieldKind::LongText
                | FieldKind::Choice { .. }
                | FieldKind::Flag => {}
            }
        }
        vec![
            ViewAction {
                fields,
                confirm: Some("Merge feature/x into main?".into()),
                primary: true,
                with_settings: true,
                ..ViewAction::new("merge_pull_request", "Merge…")
            },
            ViewAction {
                values: [("verdict".to_owned(), "approve".to_owned())].into(),
                line_comments: LineComments::Sends,
                ..ViewAction::new("submit_review", "Approve…")
            },
            ViewAction {
                values: [("verdict".to_owned(), "comment".to_owned())].into(),
                line_comments: LineComments::Requires,
                ..ViewAction::new("submit_review", "Submit review…")
            },
            ViewAction {
                confirm: Some("Delete feature/x?".into()),
                danger: true,
                disabled: Some("Protected branch".into()),
                ..ViewAction::new("delete_branch", "Delete branch…")
            },
        ]
    }

    /// One probe per block variant. The match has **no wildcard arm**, so a
    /// new block stops this module compiling until it has a probe here --
    /// `every_write_op`'s device, for the same reason: a block dropped on the
    /// hop is a tab that arrives empty.
    fn every_block() -> Vec<Block> {
        let probes = vec![
            Block::Facts {
                facts: vec![
                    Fact {
                        label: "Commit".into(),
                        value: "c90d11a".into(),
                        tone: None,
                    },
                    Fact {
                        label: "Outcome".into(),
                        value: "failed".into(),
                        tone: Some(Tone::Err),
                    },
                ],
            },
            Block::Text {
                text: "PAY-231: jitter\n\n<b>not markup</b>".into(),
                markdown: true,
            },
            Block::Entities {
                title: Some("Parents".into()),
                rows: vec![
                    ViewRow {
                        entity_id: Some("gitea:tidewater/payout-service@c90d11".into()),
                        title: "PAY-231: jitter".into(),
                        meta: Some("c90d11a".into()),
                        tone: Some(Tone::Ok),
                        chip: Some("merged".into()),
                        url: Some("https://gitea.example/c/c90d11".into()),
                        commit: None,
                    },
                    ViewRow {
                        entity_id: None,
                        title: "not mirrored".into(),
                        meta: None,
                        tone: None,
                        chip: None,
                        url: None,
                        commit: Some(CommitHint {
                            repo_url: "http://gitea:3000/tidewater/payout-service.git".into(),
                            sha: "179eb1fb0144c3b61ed482890f1a5f4e162736eb".into(),
                        }),
                    },
                ],
            },
            Block::Diff {
                files: {
                    let mut files =
                        crate::diff::parse(include_str!("../fixtures/diff/whole-commit.diff"));
                    files[0].threads = vec![LineThread {
                        side: crate::ReviewSide::Old,
                        line: 3,
                        outdated: true,
                        comments: vec![LineComment {
                            author: "priya.nair".into(),
                            body: "Bound the `jitter`.".into(),
                            at: Some("2026-08-22T09:00:00Z".into()),
                        }],
                        id: None,
                        resolved_by: None,
                        replies: false,
                    }];
                    files
                },
                truncated: true,
                commentable: Some(Commenting {
                    write_op: "submit_review".into(),
                    reply: None,
                    resolve: None,
                    head: None,
                }),
                gone: vec![FileThread {
                    path: "src/dropped.rs".into(),
                    thread: LineThread {
                        side: crate::ReviewSide::New,
                        line: 7,
                        outdated: true,
                        comments: vec![LineComment {
                            author: "jonas.becker".into(),
                            body: "Why a second retry loop?".into(),
                            at: None,
                        }],
                        id: None,
                        resolved_by: None,
                        replies: false,
                    },
                }],
            },
            Block::Log {
                text: "[12:00:01] Step 1/3\n[12:00:09] FAILED".into(),
                truncated: true,
            },
            Block::Settings {
                title: Some("Parameters".into()),
                entity: "teamcity:buildType:Payout_Build".into(),
                write_op: "set_parameter".into(),
                values: vec![Setting {
                    name: "env.RETRIES".into(),
                    value: "5".into(),
                    inherited: true,
                    secret: false,
                    editable: true,
                    description: Some("how often".into()),
                }],
            },
            Block::Actions {
                actions: every_action(),
            },
            Block::Tree {
                trail: vec![
                    Crumb {
                        name: "main".into(),
                        section: "files".into(),
                    },
                    Crumb {
                        name: "src".into(),
                        section: "files:src".into(),
                    },
                ],
                entries: vec![
                    TreeEntry {
                        name: "lib".into(),
                        kind: EntryKind::Dir,
                        size: None,
                        section: Some("files:src/lib".into()),
                    },
                    TreeEntry {
                        name: "main.rs".into(),
                        kind: EntryKind::File,
                        size: Some(120),
                        section: Some("file:src/main.rs".into()),
                    },
                    TreeEntry {
                        name: "current".into(),
                        kind: EntryKind::Symlink,
                        size: Some(7),
                        section: Some("file:src/current".into()),
                    },
                    TreeEntry {
                        name: "vendor".into(),
                        kind: EntryKind::Submodule,
                        size: None,
                        section: None,
                    },
                ],
                truncated: true,
                url: Some("https://gitea.example/o/r/src/branch/main/src".into()),
            },
            Block::File {
                trail: vec![Crumb {
                    name: "README.md".into(),
                    section: "file:README.md".into(),
                }],
                path: "README.md".into(),
                text: "# hi\n".into(),
                language: Some("markdown".into()),
                size: 5,
                truncated: true,
                binary: false,
                url: Some("https://gitea.example/o/r/src/branch/main/README.md".into()),
                sha: Some("8c2ac7f1e6c4a3b2d1e0f9a8b7c6d5e4f3a2b1c0".into()),
                commit: Some("1f0e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c".into()),
            },
        ];
        for block in &probes {
            match block {
                Block::Facts { .. }
                | Block::Text { .. }
                | Block::Entities { .. }
                | Block::Diff { .. }
                | Block::Log { .. }
                | Block::Settings { .. }
                | Block::Actions { .. }
                | Block::Tree { .. }
                | Block::File { .. } => {}
            }
        }
        probes
    }

    /// Every block, with every field, survives the hop in both directions --
    /// the view crosses a process boundary as plain data (spec §3a).
    #[test]
    fn every_block_round_trips() {
        let view = SourceView {
            actions: every_action(),
            sections: vec![
                ViewSection {
                    id: "overview".into(),
                    title: "Overview".into(),
                    count: None,
                    deferred: false,
                    blocks: every_block(),
                },
                ViewSection {
                    id: "log".into(),
                    title: "Log".into(),
                    count: Some(3),
                    deferred: true,
                    blocks: vec![],
                },
            ],
        };
        let json = serde_json::to_string(&view).unwrap();
        let back: SourceView = serde_json::from_str(&json).unwrap();
        assert_eq!(back, view);
        assert_eq!(view.sections[0].blocks.len(), 9, "a block lost its probe");
    }

    /// Issue #2030: a write the credential cannot make is drawn disabled with
    /// the reason wherever the view offers it -- the toolbar, an actions
    /// block, a settings block's values, a diff's line comments -- and every
    /// other write is left as the adapter drew it.
    #[test]
    fn a_refused_write_is_disabled_wherever_the_view_offers_it() {
        let mut view = SourceView {
            actions: every_action(),
            sections: vec![ViewSection {
                id: "overview".into(),
                title: "Overview".into(),
                count: None,
                deferred: false,
                blocks: every_block(),
            }],
        };
        let before = view.clone();
        view.refuse_writes(|op| {
            matches!(op, "submit_review" | "set_parameter" | "delete_branch")
                .then(|| "No write scope".to_owned())
        });

        let actions_block = |view: &SourceView| {
            view.sections[0]
                .blocks
                .iter()
                .find_map(|b| match b {
                    Block::Actions { actions } => Some(actions.clone()),
                    _ => None,
                })
                .unwrap()
        };
        for (drawn, was) in [
            (view.actions.clone(), before.actions.clone()),
            (actions_block(&view), actions_block(&before)),
        ] {
            let disabled: Vec<(&str, Option<&str>)> = drawn
                .iter()
                .map(|a| (a.label.as_str(), a.disabled.as_deref()))
                .collect();
            assert_eq!(
                disabled,
                [
                    ("Merge…", None),
                    ("Approve…", Some("No write scope")),
                    ("Submit review…", Some("No write scope")),
                    // A reason the adapter gave gives way: the credential's
                    // holds whatever else changes.
                    ("Delete branch…", Some("No write scope")),
                ]
            );
            assert_eq!(drawn[0], was[0], "an allowed write is untouched");
        }
        for block in &view.sections[0].blocks {
            match block {
                Block::Diff { commentable, .. } => {
                    assert_eq!(*commentable, None, "no line comments to send");
                }
                Block::Settings { values, .. } => {
                    assert!(values.iter().all(|v| !v.editable), "{values:?}");
                }
                _ => {}
            }
        }
    }

    /// A tree and a file on the wire, as the TypeScript mirror reads them:
    /// entry kinds are snake_case words, a directory has no size, and an
    /// entry that cannot be opened has no section.
    #[test]
    fn a_tree_and_a_file_have_the_wire_shape_the_shell_reads() {
        let tree = Block::Tree {
            trail: vec![Crumb {
                name: "main".into(),
                section: "files".into(),
            }],
            entries: vec![
                TreeEntry {
                    name: "src".into(),
                    kind: EntryKind::Dir,
                    size: None,
                    section: Some("files:src".into()),
                },
                TreeEntry {
                    name: "vendor".into(),
                    kind: EntryKind::Submodule,
                    size: None,
                    section: None,
                },
            ],
            truncated: false,
            url: None,
        };
        assert_eq!(
            serde_json::to_value(&tree).unwrap(),
            serde_json::json!({
                "type": "tree",
                "trail": [{ "name": "main", "section": "files" }],
                "entries": [
                    { "name": "src", "kind": "dir", "size": null, "section": "files:src" },
                    { "name": "vendor", "kind": "submodule", "size": null, "section": null }
                ],
                "truncated": false,
                "url": null
            })
        );
        let file = Block::File {
            trail: vec![],
            path: "logo.png".into(),
            text: String::new(),
            language: None,
            size: 2048,
            truncated: false,
            binary: true,
            url: None,
            sha: None,
            commit: None,
        };
        assert_eq!(
            serde_json::to_value(&file).unwrap(),
            serde_json::json!({
                "type": "file", "trail": [], "path": "logo.png", "text": "",
                "language": null, "size": 2048, "truncated": false, "binary": true,
                "url": null, "sha": null, "commit": null
            })
        );
        // A file block from before the blob was drawn reads as one without
        // it (#883).
        let old: Block = serde_json::from_value(serde_json::json!({
            "type": "file", "trail": [], "path": "a.txt", "text": "a\n",
            "language": null, "size": 2, "truncated": false, "binary": false,
            "url": null
        }))
        .expect("a file block without a sha reads");
        assert!(
            matches!(
                old,
                Block::File {
                    sha: None,
                    commit: None,
                    ..
                }
            ),
            "{old:?}"
        );
    }

    /// The wire shape the TypeScript mirror is written against: blocks are a
    /// union on `type`, tones and diff kinds are snake_case words.
    #[test]
    fn the_wire_shape_is_a_union_on_type() {
        let facts = serde_json::to_value(Block::Facts {
            facts: vec![Fact {
                label: "Outcome".into(),
                value: "failed".into(),
                tone: Some(Tone::Err),
            }],
        })
        .unwrap();
        assert_eq!(
            facts,
            serde_json::json!({
                "type": "facts",
                "facts": [{ "label": "Outcome", "value": "failed", "tone": "err" }]
            })
        );
        let mut files = crate::diff::parse(include_str!("../fixtures/diff/add.diff"));
        files[0].threads = vec![LineThread {
            side: crate::ReviewSide::New,
            line: 2,
            outdated: false,
            comments: vec![LineComment {
                author: "olu".into(),
                body: "nit".into(),
                at: None,
            }],
            id: None,
            resolved_by: None,
            replies: false,
        }];
        let diff = serde_json::to_value(Block::Diff {
            files,
            truncated: false,
            commentable: Some(Commenting {
                write_op: "submit_review".into(),
                reply: None,
                resolve: None,
                head: None,
            }),
            gone: vec![FileThread {
                path: "gone.txt".into(),
                thread: LineThread {
                    side: crate::ReviewSide::Old,
                    line: 4,
                    outdated: true,
                    comments: vec![LineComment {
                        author: "olu".into(),
                        body: "was".into(),
                        at: None,
                    }],
                    id: None,
                    resolved_by: None,
                    replies: false,
                },
            }],
        })
        .unwrap();
        assert_eq!(
            diff,
            serde_json::json!({
                "type": "diff",
                "truncated": false,
                "commentable": { "write_op": "submit_review", "reply": null, "resolve": null, "head": null },
                "gone": [{
                    "path": "gone.txt",
                    "thread": {
                        "side": "old", "line": 4, "outdated": true,
                        "comments": [{ "author": "olu", "body": "was", "at": null }],
                        "id": null, "resolved_by": null, "replies": false
                    }
                }],
                "files": [{
                    "path": "added.txt",
                    "old_path": null,
                    "change": "added",
                    "additions": 2,
                    "deletions": 0,
                    "truncated": false,
                    "blob": "5786b13",
                    "hunks": [{
                        "header": "@@ -0,0 +1,2 @@",
                        "lines": [
                            { "kind": "add", "old": null, "new": 1, "text": "brand" },
                            { "kind": "add", "old": null, "new": 2, "text": "new" }
                        ]
                    }],
                    "threads": [{
                        "side": "new", "line": 2, "outdated": false,
                        "comments": [{ "author": "olu", "body": "nit", "at": null }],
                        "id": null, "resolved_by": null, "replies": false
                    }]
                }]
            })
        );
        // A diff drawn before #709 -- no threads, no `commentable` -- decodes
        // as one nobody commented on and nobody may.
        let before: Block = serde_json::from_value(serde_json::json!({
            "type": "diff", "truncated": false,
            "files": [{ "path": "a", "old_path": null, "change": "added", "additions": 0,
                        "deletions": 0, "truncated": false, "hunks": [] }]
        }))
        .unwrap();
        let Block::Diff {
            files,
            commentable,
            gone,
            ..
        } = before
        else {
            panic!("a diff decodes as a diff");
        };
        assert!(files[0].threads.is_empty() && commentable.is_none() && gone.is_empty());
        // A row's commit hint is absent on the wire when it is `None`, so the
        // shell's row keeps the shape #661 froze; one sent without it decodes.
        let row = ViewRow {
            entity_id: None,
            title: "t".into(),
            meta: None,
            tone: None,
            chip: None,
            url: None,
            commit: None,
        };
        let wire = serde_json::to_value(&row).unwrap();
        assert_eq!(
            wire,
            serde_json::json!({ "entity_id": null, "title": "t", "meta": null, "tone": null,
                                "chip": null, "url": null })
        );
        assert_eq!(serde_json::from_value::<ViewRow>(wire).unwrap(), row);
        assert_eq!(
            serde_json::to_value(Block::Settings {
                title: None,
                entity: "teamcity:buildType:X".into(),
                write_op: "set_parameter".into(),
                values: vec![],
            })
            .unwrap()["type"],
            "settings"
        );
        let merge = serde_json::json!({
            "write_op": "merge_pull_request",
            "label": "Merge…",
            "fields": [
                { "name": "style", "label": "Style",
                  "kind": { "type": "choice", "options": [
                      { "value": "merge", "label": "Merge commit" },
                      { "value": "squash", "label": "Squash" } ] },
                  "value": null, "required": true, "placeholder": null },
                { "name": "delete_branch", "label": "Delete the branch",
                  "kind": { "type": "flag" },
                  "value": "true", "required": false, "placeholder": null },
                { "name": "title", "label": "Title",
                  "kind": { "type": "text", "suggestions": [] },
                  "value": null, "required": false, "placeholder": "a line" },
                { "name": "body", "label": "Description",
                  "kind": { "type": "long_text" },
                  "value": null, "required": false, "placeholder": null }
            ],
            "confirm": "Merge?",
            "primary": true,
            "danger": false,
            "disabled": "Not mergeable",
            "with_settings": false,
            "values": {},
            "entity": null,
            "line_comments": "none"
        });
        let action = ViewAction {
            fields: vec![
                ActionField {
                    required: true,
                    ..ActionField::new(
                        "style",
                        "Style",
                        FieldKind::Choice {
                            options: vec![
                                ChoiceOption::new("merge", "Merge commit"),
                                ChoiceOption::new("squash", "Squash"),
                            ],
                        },
                    )
                },
                ActionField {
                    value: Some("true".into()),
                    ..ActionField::new("delete_branch", "Delete the branch", FieldKind::Flag)
                },
                ActionField {
                    placeholder: Some("a line".into()),
                    ..ActionField::new(
                        "title",
                        "Title",
                        FieldKind::Text {
                            suggestions: vec![],
                        },
                    )
                },
                ActionField::new("body", "Description", FieldKind::LongText),
            ],
            confirm: Some("Merge?".into()),
            primary: true,
            disabled: Some("Not mergeable".into()),
            ..ViewAction::new("merge_pull_request", "Merge…")
        };
        assert_eq!(
            serde_json::to_value(Block::Actions {
                actions: vec![action.clone()],
            })
            .unwrap(),
            serde_json::json!({ "type": "actions", "actions": [merge] })
        );
        // The toolbar is the view's own list, and the flags and the field's
        // optional parts may be left out: an action drawn with only its op,
        // label and sentence, and a view with no actions at all, decode.
        assert_eq!(
            serde_json::from_value::<ViewAction>(serde_json::json!({
                "write_op": "run_build", "label": "Run", "confirm": null
            }))
            .unwrap(),
            ViewAction::new("run_build", "Run")
        );
        // Fixed answers the source sets (#705): an object of names to text,
        // never drawn, so one op can be two actions (*Approve…*, *Request
        // changes…*).
        let approve = ViewAction {
            values: [("verdict".to_owned(), "approve".to_owned())].into(),
            ..ViewAction::new("submit_review", "Approve…")
        };
        assert_eq!(
            serde_json::to_value(&approve).unwrap()["values"],
            serde_json::json!({ "verdict": "approve" })
        );
        // What an action does with the pending line comments (#709): a
        // snake_case word, `none` when absent.
        for (use_, word) in [
            (LineComments::None, "none"),
            (LineComments::Sends, "sends"),
            (LineComments::Requires, "requires"),
        ] {
            let action = ViewAction {
                line_comments: use_,
                ..ViewAction::new("submit_review", "Submit review…")
            };
            assert_eq!(
                serde_json::to_value(&action).unwrap()["line_comments"],
                word
            );
        }
        assert_eq!(
            serde_json::from_value::<ActionField>(serde_json::json!({
                "name": "body", "label": "Body", "kind": { "type": "long_text" }
            }))
            .unwrap(),
            ActionField::new("body", "Body", FieldKind::LongText)
        );
        assert_eq!(
            serde_json::from_value::<FieldKind>(serde_json::json!({ "type": "text" })).unwrap(),
            FieldKind::Text {
                suggestions: vec![]
            }
        );
        // A write whose target is not the view's entity names it (#707): a
        // branch's *New branch…* is created in the branch's repository.
        assert_eq!(
            serde_json::to_value(ViewAction {
                entity: Some("gitea:o/r".into()),
                ..ViewAction::new("create_branch", "New branch…")
            })
            .unwrap()["entity"],
            "gitea:o/r"
        );
        let view: SourceView =
            serde_json::from_value(serde_json::json!({ "sections": [] })).unwrap();
        assert!(view.actions.is_empty());
        assert_eq!(
            serde_json::to_value(SourceView {
                sections: vec![],
                actions: vec![action],
            })
            .unwrap(),
            serde_json::json!({ "sections": [], "actions": [merge] })
        );
    }
}
