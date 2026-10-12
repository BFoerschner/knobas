//! The source-view vocabulary: what an adapter draws for one of its own kinds
//! when a detail opens (ADR-0020, `CONTEXT.md` **Source view**).
//!
//! A view is a list of [`ViewSection`]s, each a list of typed [`Block`]s from
//! one fixed vocabulary -- facts, text, an entity list, a diff, a log,
//! settings. The shell draws the vocabulary and never learns which adapter
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
                    },
                    ViewRow {
                        entity_id: None,
                        title: "not mirrored".into(),
                        meta: None,
                        tone: None,
                        chip: None,
                        url: None,
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
        ];
        for block in &probes {
            match block {
                Block::Facts { .. }
                | Block::Text { .. }
                | Block::Entities { .. }
                | Block::Diff { .. }
                | Block::Log { .. }
                | Block::Settings { .. } => {}
            }
        }
        probes
    }

    /// Every block, with every field, survives the hop in both directions --
    /// the view crosses a process boundary as plain data (spec §3a).
    #[test]
    fn every_block_round_trips() {
        let view = SourceView {
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
        assert_eq!(view.sections[0].blocks.len(), 6, "a block lost its probe");
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
    }
}
