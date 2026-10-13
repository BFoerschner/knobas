//! The source-view vocabulary: what an adapter draws for one of its own kinds
//! when a detail opens (ADR-0020, `CONTEXT.md` **Source view**).
//!
//! A view is a list of [`ViewSection`]s, each a list of typed [`Block`]s from
//! one fixed vocabulary -- facts, text, an entity list, a diff, a log,
//! settings, actions. The shell draws the vocabulary and never learns which adapter
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
pub use crate::diff::{DiffChange, DiffFile, DiffLine, DiffLineKind, Hunk};

/// What [`Source::view`](crate::Source::view) answers for one entity: its
/// sections, in the order the shell draws them as tabs. The first is the one a
/// detail opens on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
}

/// One tab of a view.
///
/// A **deferred** section is expensive to read -- a pull request's files, a
/// build's log. The whole-view read carries its `id`, `title` and `count` with
/// `blocks` empty; the shell reads its blocks when the reader first opens the
/// tab, by calling `view` again with the section's `id` (ADR-0020 rule 3).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    /// Label and value pairs: a commit's sha and author, a build's outcome.
    Facts { facts: Vec<Fact> },
    /// Prose: a commit message, a pull request's description. `markdown`
    /// says the source writes Markdown there; the shell still draws it as
    /// text, never as markup (ADR-0020 rule 5).
    Text { text: String, markdown: bool },
    /// A list of things, each of which may be an address (ADR-0020 rule 7):
    /// a commit's parents, a pull request's commits, a build's changes.
    Entities {
        title: Option<String>,
        rows: Vec<ViewRow>,
    },
    /// A diff, parsed by [`crate::diff`] and capped there. `truncated` says
    /// the per-view cap cut it: some file after the cut carries its counts and
    /// no lines.
    Diff {
        files: Vec<DiffFile>,
        truncated: bool,
    },
    /// Plain text with meaningful lines, drawn monospace: a build log's tail.
    /// `truncated` says this is the end of something longer.
    Log { text: String, truncated: bool },
    /// Named values a reader may be allowed to change (ADR-0020 rule 4): a
    /// build configuration's parameters. `entity` is what an edit targets and
    /// `write_op` is the [`WriteOp`](crate::WriteOp) identifier that changes a
    /// value; a view never writes.
    Settings {
        title: Option<String>,
        entity: String,
        write_op: String,
        values: Vec<Setting>,
    },
    /// Writes a reader can start from this section, one button each (issue
    /// #667, ADR-0020 rule 4). An action of the whole entity belongs in
    /// [`SourceView::actions`], the toolbar (issue #702); this block is for
    /// one that belongs to one section, and never repeats a toolbar action.
    ///
    /// Whether an action can be taken is the source's answer, read in the
    /// adapter ([`ViewAction::disabled`]); the shell never decides it by
    /// reading a fact's label or the payload (ADR-0007). The shell
    /// additionally draws an action only where the source's descriptor
    /// declares its `write_op`, and queues it through the write queue against
    /// the entity the view is of. A view never writes.
    Actions { actions: Vec<ViewAction> },
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
/// `danger` queues on the button, with no dialog.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
        }
    }
}

/// One input of a [`ViewAction`]'s dialog. The reader's answer reaches the
/// shell's payload table as text under `name`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldKind {
    /// One line. `suggestions` are offered as the reader types and none is
    /// picked: a run's branch suggests the branches recent builds ran on.
    Text {
        #[serde(default)]
        suggestions: Vec<String>,
    },
    /// Several lines of Markdown: a description, a comment. Drawn as a plain
    /// text area; the shell never renders it (ADR-0020 rule 5).
    LongText,
    /// One of `options`, picked.
    Choice { options: Vec<ChoiceOption> },
    /// A checkbox.
    Flag,
}

/// One option of a [`FieldKind::Choice`]: the `value` the op takes, and the
/// `label` the reader sees.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
                files: crate::diff::parse(include_str!("../fixtures/diff/whole-commit.diff")),
                truncated: true,
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
        ];
        for block in &probes {
            match block {
                Block::Facts { .. }
                | Block::Text { .. }
                | Block::Entities { .. }
                | Block::Diff { .. }
                | Block::Log { .. }
                | Block::Settings { .. }
                | Block::Actions { .. } => {}
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
        assert_eq!(view.sections[0].blocks.len(), 7, "a block lost its probe");
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
        let diff = serde_json::to_value(Block::Diff {
            files: crate::diff::parse(include_str!("../fixtures/diff/add.diff")),
            truncated: false,
        })
        .unwrap();
        assert_eq!(
            diff,
            serde_json::json!({
                "type": "diff",
                "truncated": false,
                "files": [{
                    "path": "added.txt",
                    "old_path": null,
                    "change": "added",
                    "additions": 2,
                    "deletions": 0,
                    "truncated": false,
                    "hunks": [{
                        "header": "@@ -0,0 +1,2 @@",
                        "lines": [
                            { "kind": "add", "old": null, "new": 1, "text": "brand" },
                            { "kind": "add", "old": null, "new": 2, "text": "new" }
                        ]
                    }]
                }]
            })
        );
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
            "with_settings": false
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
